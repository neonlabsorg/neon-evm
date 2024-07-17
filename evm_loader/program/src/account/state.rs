use std::cell::{Ref, RefMut};
use std::mem::{align_of, size_of, ManuallyDrop};
use std::ptr::{addr_of, read_unaligned};

use crate::account_storage::AccountStorage;
use crate::allocator::acc_allocator;
use crate::config::DEFAULT_CHAIN_ID;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::database::Database;
use crate::evm::tracing::EventListener;
use crate::evm::Machine;
use crate::executor::ExecutorStateData;
use crate::types::boxx::{boxx, Boxx};
use crate::types::{
    read_raw_utils::{read_vec, ReconstructRaw},
    AccessListTx, Address, LegacyTx, Transaction, TransactionPayload, TreeMap,
};

use ethnum::U256;
use solana_program::hash::Hash;
use solana_program::system_program;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use super::{
    AccountHeader, AccountsDB, BalanceAccount, ContractAccount, Holder, OperatorBalanceAccount,
    StateFinalizedAccount, StorageCell, ACCOUNT_PREFIX_LEN, TAG_ACCOUNT_BALANCE,
    TAG_ACCOUNT_CONTRACT, TAG_HOLDER, TAG_STATE, TAG_STATE_FINALIZED, TAG_STORAGE_CELL,
};

#[derive(PartialEq, Eq)]
pub enum AccountsStatus {
    Ok,
    NeedRestart,
}

#[derive(Clone, PartialEq, Eq)]
#[repr(C)]
enum AccountRevision {
    Revision(u32),
    Hash([u8; 32]),
}

impl AccountRevision {
    pub fn new(program_id: &Pubkey, info: &AccountInfo) -> Self {
        if (info.owner != program_id) && !system_program::check_id(info.owner) {
            if crate::config::NO_UPDATE_TRACKING_OWNERS
                .binary_search(info.owner)
                .is_ok()
            {
                return AccountRevision::Hash(Hash::default().to_bytes());
            }

            let hash = solana_program::hash::hashv(&[
                info.owner.as_ref(),
                &info.lamports().to_le_bytes(),
                &info.data.borrow(),
            ]);

            return AccountRevision::Hash(hash.to_bytes());
        }

        match crate::account::tag(program_id, info) {
            Ok(TAG_STORAGE_CELL) => {
                let cell = StorageCell::from_account(program_id, info.clone()).unwrap();
                Self::Revision(cell.revision())
            }
            Ok(TAG_ACCOUNT_CONTRACT) => {
                let contract = ContractAccount::from_account(program_id, info.clone()).unwrap();
                Self::Revision(contract.revision())
            }
            Ok(TAG_ACCOUNT_BALANCE) => {
                let balance = BalanceAccount::from_account(program_id, info.clone()).unwrap();
                Self::Revision(balance.revision())
            }
            _ => Self::Revision(0),
        }
    }
}

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C)]
struct Data {
    pub owner: Pubkey,
    pub transaction: Transaction,
    /// Ethereum transaction caller address
    pub origin: Address,
    /// Stored revision
    pub revisions: TreeMap<Pubkey, AccountRevision>,
    /// Accounts that been read during the transaction
    pub touched_accounts: TreeMap<Pubkey, u64>,
    /// Ethereum transaction gas used and paid
    pub gas_used: U256,
    /// Steps executed in the transaction
    pub steps_executed: u64,
}

// Stores relative offsets for the corresponding objects as allocated by the AccountAllocator.
#[allow(clippy::struct_field_names)]
#[repr(C, packed)]
struct Header {
    pub executor_state_offset: usize,
    pub evm_machine_offset: usize,
    pub data_offset: usize,
}
impl AccountHeader for Header {
    const VERSION: u8 = 0;
}

pub struct StateAccount<'a> {
    account: AccountInfo<'a>,
    // ManuallyDrop to ensure Data is not dropped when StateAccount
    // is being dropped (between iterations).
    data: ManuallyDrop<Boxx<Data>>,
}

type StateAccountCoreApiView = (Transaction, Pubkey, Address, Vec<Pubkey>, u64);

const BUFFER_OFFSET: usize = ACCOUNT_PREFIX_LEN + size_of::<Header>();

impl<'a> StateAccount<'a> {
    #[must_use]
    pub fn into_account(self) -> AccountInfo<'a> {
        self.account
    }

    pub fn from_account(program_id: &Pubkey, account: &AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, account, TAG_STATE)?;

        let header = super::header::<Header>(account);
        let data_ptr = unsafe {
            // Data is more-strictly aligned, but it's safe because we previously initiated it at the exact address.
            #[allow(clippy::cast_ptr_alignment)]
            account
                .data
                .borrow()
                .as_ptr()
                .add(header.data_offset)
                .cast::<Data>()
                .cast_mut()
        };

        Ok(Self {
            account: account.clone(),
            data: ManuallyDrop::new(unsafe { Boxx::from_raw_in(data_ptr, acc_allocator()) }),
        })
    }

    pub fn new(
        program_id: &Pubkey,
        info: AccountInfo<'a>,
        accounts: &AccountsDB<'a>,
        origin: Address,
        transaction: Boxx<Transaction>,
    ) -> Result<Self> {
        let owner = match super::tag(program_id, &info)? {
            TAG_HOLDER => {
                let holder = Holder::from_account(program_id, info.clone())?;
                holder.validate_owner(accounts.operator())?;
                holder.owner()
            }
            TAG_STATE_FINALIZED => {
                let finalized = StateFinalizedAccount::from_account(program_id, info.clone())?;
                finalized.validate_owner(accounts.operator())?;
                finalized.validate_trx(&transaction)?;
                finalized.owner()
            }
            tag => return Err(Error::AccountInvalidTag(*info.key, tag)),
        };

        let revisions_it = accounts.into_iter().map(|account| {
            let revision = AccountRevision::new(program_id, account);
            (*account.key, revision)
        });

        // Construct TreeMap (based on the AccountAllocator) from constructed revisions_it.
        let mut revisions = TreeMap::<Pubkey, AccountRevision>::new();
        for (key, rev) in revisions_it {
            revisions.insert(key, &rev);
        }

        let data = boxx(Data {
            owner,
            transaction: unsafe { std::ptr::read(Boxx::into_raw(transaction)) },
            origin,
            revisions,
            touched_accounts: TreeMap::new(),
            gas_used: U256::ZERO,
            steps_executed: 0_u64,
        });

        let data_offset = {
            let account_data_ptr = info.data.borrow().as_ptr();
            let data_obj_addr = addr_of!(*data).cast::<u8>();
            let data_offset = unsafe { data_obj_addr.offset_from(account_data_ptr) };
            #[allow(clippy::cast_sign_loss)]
            let data_offset = data_offset as usize;
            data_offset
        };

        super::set_tag(program_id, &info, TAG_STATE, Header::VERSION)?;
        {
            // Set header
            let mut header = super::header_mut::<Header>(&info);
            header.executor_state_offset = 0;
            header.evm_machine_offset = 0;
            header.data_offset = data_offset;
        }

        Ok(Self {
            account: info,
            data: ManuallyDrop::new(data),
        })
    }

    pub fn restore(
        program_id: &Pubkey,
        info: &AccountInfo<'a>,
        accounts: &AccountsDB,
    ) -> Result<(Self, AccountsStatus)> {
        let mut status = AccountsStatus::Ok;
        let mut state = Self::from_account(program_id, info)?;

        let is_touched_account = |key: &Pubkey| -> bool {
            state
                .data
                .touched_accounts
                .get(key)
                .map(|counter| counter > &1)
                .is_some()
        };

        let touched_accounts = accounts.into_iter().filter(|a| is_touched_account(a.key));
        for account in touched_accounts {
            let account_revision = AccountRevision::new(program_id, account);
            let revision_entry = state.data.revisions.get(account.key).unwrap();

            if revision_entry != &account_revision {
                log_data(&[b"INVALID_REVISION", account.key.as_ref()]);
                status = AccountsStatus::NeedRestart;
                break;
            }
        }

        if status == AccountsStatus::NeedRestart {
            // update all accounts revisions
            for account in accounts {
                let account_revision = AccountRevision::new(program_id, account);
                state.data.revisions.insert(*account.key, &account_revision);
            }
        }

        Ok((state, status))
    }

    pub fn finalize(self, program_id: &Pubkey) -> Result<()> {
        debug_print!("Finalize Storage {}", self.account.key);

        // Change tag to finalized
        StateFinalizedAccount::convert_from_state(program_id, self)?;

        Ok(())
    }

    pub fn update_touched_accounts(&mut self, touched: &TreeMap<Pubkey, u64>) -> Result<()> {
        for key in touched.keys() {
            let counter = *touched.get(key).unwrap();
            let prev_cnt = self.data.touched_accounts.get(key).map_or(0, |v| *v);
            self.data.touched_accounts.insert(
                *key,
                &prev_cnt
                    .checked_add(counter)
                    .ok_or(Error::IntegerOverflow)?,
            );
        }

        Ok(())
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Pubkey> {
        self.data.revisions.keys()
    }

    #[must_use]
    pub fn buffer(&self) -> Ref<[u8]> {
        let data = self.account.try_borrow_data().unwrap();
        Ref::map(data, |d| &d[BUFFER_OFFSET..])
    }

    #[must_use]
    pub fn buffer_mut(&mut self) -> RefMut<[u8]> {
        let data = self.account.data.borrow_mut();
        RefMut::map(data, |d| &mut d[BUFFER_OFFSET..])
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        self.data.owner
    }

    #[must_use]
    pub fn trx(&self) -> &Transaction {
        &self.data.transaction
    }

    #[must_use]
    pub fn trx_origin(&self) -> Address {
        self.data.origin
    }

    #[must_use]
    pub fn trx_chain_id(&self, backend: &impl AccountStorage) -> u64 {
        self.data
            .transaction
            .chain_id()
            .unwrap_or_else(|| backend.default_chain_id())
    }

    #[must_use]
    pub fn gas_used(&self) -> U256 {
        self.data.gas_used
    }

    #[must_use]
    pub fn gas_available(&self) -> U256 {
        self.trx().gas_limit().saturating_sub(self.gas_used())
    }

    fn use_gas(&mut self, amount: U256) -> Result<U256> {
        if amount == U256::ZERO {
            return Ok(U256::ZERO);
        }

        let total_gas_used = self.data.gas_used.saturating_add(amount);
        let gas_limit = self.trx().gas_limit();

        if total_gas_used > gas_limit {
            return Err(Error::OutOfGas(gas_limit, total_gas_used));
        }

        self.data.gas_used = total_gas_used;

        amount
            .checked_mul(self.trx().gas_price())
            .ok_or(Error::IntegerOverflow)
    }

    pub fn consume_gas(
        &mut self,
        amount: U256,
        receiver: Option<OperatorBalanceAccount>,
    ) -> Result<()> {
        let tokens = self.use_gas(amount)?;
        if tokens == U256::ZERO {
            return Ok(());
        }

        let mut operator_balance = receiver.ok_or(Error::OperatorBalanceMissing)?;

        let trx_chain_id = self.trx().chain_id().unwrap_or(DEFAULT_CHAIN_ID);
        if operator_balance.chain_id() != trx_chain_id {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        operator_balance.mint(tokens)
    }

    pub fn refund_unused_gas(&mut self, origin: &mut BalanceAccount) -> Result<()> {
        let trx_chain_id = self.trx().chain_id().unwrap_or(DEFAULT_CHAIN_ID);

        assert!(origin.chain_id() == trx_chain_id);
        assert!(origin.address() == self.trx_origin());

        let unused_gas = self.gas_available();
        let tokens = self.use_gas(unused_gas)?;
        origin.mint(tokens)
    }

    #[must_use]
    pub fn steps_executed(&self) -> u64 {
        self.data.steps_executed
    }

    pub fn reset_steps_executed(&mut self) {
        self.data.steps_executed = 0;
    }

    pub fn increment_steps_executed(&mut self, steps: u64) -> Result<()> {
        self.data.steps_executed = self
            .data
            .steps_executed
            .checked_add(steps)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }
}

// Implementation of functional to save/restore persistent state of iterative transactions.
impl<'a> StateAccount<'a> {
    pub fn alloc_executor_state(&self, data: Boxx<ExecutorStateData>) -> Result<()> {
        let offset = self.leak_and_offset(data);
        let mut header = super::header_mut::<Header>(&self.account);
        header.executor_state_offset = offset;
        Ok(())
    }

    pub fn alloc_evm<B: Database, T: EventListener>(&self, evm: Boxx<Machine<B, T>>) -> Result<()> {
        let offset = self.leak_and_offset(evm);
        let mut header = super::header_mut::<Header>(&self.account);
        header.evm_machine_offset = offset;
        Ok(())
    }

    /// Leak the Box's underlying data and returns offset from the account data start.
    fn leak_and_offset<T>(&self, object: Boxx<T>) -> usize {
        let data_ptr = self.account.data.borrow().as_ptr();
        unsafe {
            // allocator_api2 does not expose Box::leak (private associated fn).
            // We avoid drop of persistent object by leaking via Box::into_raw.
            let obj_addr = Boxx::into_raw(object).cast_const().cast::<u8>();

            let offset = obj_addr.offset_from(data_ptr);
            assert!(offset > 0);
            #[allow(clippy::cast_sign_loss)]
            let offset = offset as usize;
            offset
        }
    }

    #[must_use]
    pub fn read_evm<B: Database, T: EventListener>(&self) -> ManuallyDrop<Boxx<Machine<B, T>>> {
        let header = super::header::<Header>(&self.account);
        self.map_obj(header.evm_machine_offset)
    }

    #[must_use]
    pub fn read_executor_state(&self) -> ManuallyDrop<Boxx<ExecutorStateData>> {
        let header = super::header::<Header>(&self.account);
        self.map_obj(header.executor_state_offset)
    }

    fn map_obj<T>(&self, offset: usize) -> ManuallyDrop<Boxx<T>> {
        let data = self.account.data.borrow().as_ptr();
        unsafe {
            let ptr = data.add(offset).cast_mut();
            assert_eq!(ptr.align_offset(align_of::<T>()), 0);
            let data_ptr = ptr.cast::<T>();

            ManuallyDrop::new(Boxx::from_raw_in(data_ptr, acc_allocator()))
        }
    }
}

impl<'a> StateAccount<'a> {
    /// Implementation to squeeze bits of information from the state account.
    /// N.B.
    /// 1. `StateAccount` contains objects and pointers allocated by the state account allocator, so reading
    ///     objects inside requires jumping on the offset (between the real account address as allocated by the
    ///     current allocator) and "intended" address of the first account as provided by the Solana runtime.
    /// 2. `addr_of!` and `read_unaligned` is heavily used to facilitate the reading of fields by raw pointers.
    /// 3. There are upcasts from *const u8 to *const T, but since T was allocated by the allocator previously,
    ///     it has the correct alignment and the upcast is sound.
    #[allow(clippy::cast_ptr_alignment)]
    pub fn get_state_account_view(
        program_id: &Pubkey,
        account: &AccountInfo<'a>,
    ) -> Result<StateAccountCoreApiView> {
        super::validate_tag(program_id, account, TAG_STATE)?;

        let header = super::header::<Header>(account);
        let memory_space_delta = {
            account.data.borrow().as_ptr() as isize
                - isize::try_from(crate::allocator::STATE_ACCOUNT_DATA_ADDRESS)?
        };
        // Pointer to the Data is needed to get pointers to the fields in a safe way (using addr_of!).
        let data_ptr = unsafe {
            account
                .data
                .borrow()
                .as_ptr()
                .add(header.data_offset)
                .cast::<Data>()
                .cast_mut()
        };

        unsafe {
            // Reading full `Transaction`.
            let transaction_ptr = addr_of!((*data_ptr).transaction);
            // Memory layout for transaction payload is: tag of enum's variant (usize) followed by the variant value.
            let tx_payload_ptr = addr_of!((*transaction_ptr).transaction).cast::<usize>();
            let payload_ptr = tx_payload_ptr.add(1);

            // TODO: ugly manual piece of code that likely can be substituted with the macro.
            let tx_payload: TransactionPayload = match read_unaligned(tx_payload_ptr) {
                0 => {
                    let legacy_payload_ptr = payload_ptr.cast::<LegacyTx>();
                    TransactionPayload::Legacy(LegacyTx::build(
                        legacy_payload_ptr,
                        memory_space_delta,
                    ))
                }
                1 => {
                    let access_list_tx_ptr = payload_ptr.cast::<AccessListTx>();
                    TransactionPayload::AccessList(AccessListTx::build(
                        access_list_tx_ptr,
                        memory_space_delta,
                    ))
                }
                _ => {
                    return Err(Error::Custom(
                        "Incorrect transaction payload type.".to_owned(),
                    ));
                }
            };

            let byte_len = read_unaligned(addr_of!((*transaction_ptr).byte_len));
            let hash = read_unaligned(addr_of!((*transaction_ptr).hash));
            let signed_hash = read_unaligned(addr_of!((*transaction_ptr).signed_hash));
            let tx = Transaction {
                transaction: tx_payload,
                byte_len,
                hash,
                signed_hash,
            };

            // Reading parts of `StateAccount`.
            let owner = read_unaligned(addr_of!((*data_ptr).owner));
            let origin = read_unaligned(addr_of!((*data_ptr).origin));
            let keys_ptr = addr_of!((*data_ptr).revisions).cast::<usize>();
            let accounts = read_vec(keys_ptr, memory_space_delta);

            let steps = read_unaligned(addr_of!((*data_ptr).steps_executed));

            Ok((tx, owner, origin, accounts, steps))
        }
    }
}
