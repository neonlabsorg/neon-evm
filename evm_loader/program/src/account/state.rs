use std::cell::{Ref, RefMut};
use std::mem::{align_of, size_of, ManuallyDrop};

use crate::account_storage::AccountStorage;
use crate::allocator::acc_allocator;
use crate::config::DEFAULT_CHAIN_ID;
use crate::error::{Error, Result};

use crate::evm::database::Database;
use crate::evm::tracing::EventListener;
use crate::evm::Machine;
use crate::executor::ExecutorStateData;
use crate::types::boxx::{boxx, Boxx};
use crate::types::{Address, Transaction, TreeMap};
use ethnum::U256;
use linked_list_allocator::Heap;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use super::{
    revision, AccountHeader, AccountsDB, BalanceAccount, Holder, StateFinalizedAccount,
    ACCOUNT_PREFIX_LEN, TAG_HOLDER, TAG_STATE, TAG_STATE_FINALIZED,
};

#[derive(PartialEq, Eq)]
pub enum AccountsStatus {
    Ok,
    RevisionChanged,
}

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C)]
struct Data {
    pub owner: Pubkey,
    pub transaction: Transaction,
    /// Ethereum transaction caller address
    pub origin: Address,
    /// Stored accounts
    pub revisions: TreeMap<Pubkey, u32>,
    /// Ethereum transaction gas used and paid
    pub gas_used: U256,
    /// Steps executed in the transaction
    pub steps_executed: u64,
}

// Stores relative offsets for the corresponding objects as allocated by the AccountAllocator.
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

const BUFFER_OFFSET: usize = ACCOUNT_PREFIX_LEN + size_of::<Header>();
// A valid offset for Heap object alignment in the real memory space.
// This offset is valid when State/Holder goes first in the list of accounts of instruction.
// 80 is chosen as a smallest offset which is bigger than any header (for state or holder).
// P.S. Should be pub because Allocator needs to know the offset that points to the Heap.
// TODO FIX!!!!!!!!!!!!!!!
pub const HEAP_OBJECT_OFFSET: usize = 8000;

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

        // Initialize the heap before any allocations.
        //Self::init_heap(&info)?;

        // todo: get revision from account
        let revisions_it = accounts.into_iter().map(|account| {
            let revision = revision(program_id, account).unwrap_or(0);
            (*account.key, revision)
        });

        // Construct TreeMap (based on the AccountAllocator) from constructed revisions_it.
        let mut revisions = TreeMap::<Pubkey, u32>::new();
        for (key, rev) in revisions_it {
            revisions.insert(key, &rev);
        }

        let data = boxx(Data {
            owner,
            transaction: unsafe { std::ptr::read(Boxx::into_raw(transaction)) },
            origin,
            revisions,
            gas_used: U256::ZERO,
            steps_executed: 0_u64,
        });

        let data_offset = {
            let account_data_ptr = info.data.borrow().as_ptr();
            let data_obj_addr = std::ptr::addr_of!(*data).cast::<u8>();
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
        let mut state = Self::from_account(program_id, info)?;
        let mut status = AccountsStatus::Ok;

        for account in accounts {
            let account_revision = revision(program_id, account).unwrap_or(0);
            let stored_revision = state
                .data
                .revisions
                .get(account.key)
                .map_or(account_revision, |rev| *rev);

            if stored_revision != account_revision {
                status = AccountsStatus::RevisionChanged;
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

    pub fn consume_gas(&mut self, amount: U256, receiver: &mut BalanceAccount) -> Result<()> {
        if amount == U256::ZERO {
            return Ok(());
        }

        let trx_chain_id = self.trx().chain_id().unwrap_or(DEFAULT_CHAIN_ID);
        if receiver.chain_id() != trx_chain_id {
            return Err(Error::GasReceiverInvalidChainId);
        }

        let total_gas_used = self.data.gas_used.saturating_add(amount);
        let gas_limit = self.trx().gas_limit();

        if total_gas_used > gas_limit {
            return Err(Error::OutOfGas(gas_limit, total_gas_used));
        }

        self.data.gas_used = total_gas_used;

        let tokens = amount
            .checked_mul(self.trx().gas_price())
            .ok_or(Error::IntegerOverflow)?;
        receiver.mint(tokens)
    }

    pub fn refund_unused_gas(&mut self, origin: &mut BalanceAccount) -> Result<()> {
        let trx_chain_id = self.trx().chain_id().unwrap_or(DEFAULT_CHAIN_ID);

        assert!(origin.chain_id() == trx_chain_id);
        assert!(origin.address() == self.trx_origin());

        let unused_gas = self.gas_available();
        self.consume_gas(unused_gas, origin)
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

    /// Initializes the heap using the whole account data space right after the Header section.
    /// After this, the persistent objects can be allocated in the account data.
    ///
    /// N.B. No ownership checks are performed, it's a caller's responsibility.
    /// TODO: This piece of should be moved to mod.rs.
    pub fn init_heap(info: &AccountInfo<'a>) -> Result<()> {
        let data = info.data.borrow();
        // Init the heap at the predefined address (right after the header with proper alignment).
        let heap_ptr = data.as_ptr().wrapping_add(HEAP_OBJECT_OFFSET).cast_mut();
        assert_eq!(heap_ptr.align_offset(align_of::<Heap>()), 0);
        unsafe {
            // First, zero out underlying bytes of the future heap representation.
            heap_ptr.write_bytes(0, size_of::<Heap>());
            // Calculate the bottom of the heap, right after the Heap object.
            let heap_bottom = heap_ptr.add(size_of::<Heap>());
            // Size is equal to account data length minus the length of prefix.
            let heap_size = info
                .data_len()
                .saturating_sub(HEAP_OBJECT_OFFSET + size_of::<Heap>());
            // Cast to reference and init.
            // Zeroed memory is a valid representation of the Heap and hence we can safely do it.
            // That's a safety reason we zeroed the memory above.
            #[allow(clippy::cast_ptr_alignment)]
            let heap = &mut *(heap_ptr.cast::<Heap>());
            heap.init(heap_bottom, heap_size);
        };
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
        assert!(header.evm_machine_offset > HEAP_OBJECT_OFFSET);
        self.map_obj(header.evm_machine_offset)
    }

    #[must_use]
    pub fn read_executor_state(&self) -> ManuallyDrop<Boxx<ExecutorStateData>> {
        let header = super::header::<Header>(&self.account);
        assert!(header.executor_state_offset > HEAP_OBJECT_OFFSET);
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
