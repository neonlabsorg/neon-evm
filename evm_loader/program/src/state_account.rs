#[cfg(target_os = "solana")]
use {
    crate::types::{Address, Transaction},
    ethnum::U256,
    solana_program::clock::Clock,
    solana_program::sysvar::Sysvar,
};

use {
    crate::account::EthereumAccount,
    crate::account::Holder,
    crate::error::Error,
    solana_program::account_info::AccountInfo,
    std::cell::{Ref, RefMut},
};

use crate::account::{FinalizedState, Incinerator, Operator, State};
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

const ACCOUNT_CHUNK_LEN: usize = 1 + 1 + 32;

pub enum Deposit<'a> {
    ReturnToOperator(Operator<'a>),
    Burn(Incinerator<'a>),
}

pub struct BlockedAccountMeta {
    pub key: Pubkey,
    pub exists: bool,
    pub is_writable: bool,
}

pub type BlockedAccounts = Vec<BlockedAccountMeta>;

impl<'a> FinalizedState<'a> {
    #[must_use]
    pub fn is_outdated(&self, transaction_hash: &[u8; 32]) -> bool {
        self.transaction_hash.ne(transaction_hash)
    }
}

impl<'a> State<'a> {
    #[cfg(target_os = "solana")]
    pub fn new(
        program_id: &'a Pubkey,
        info: &'a AccountInfo<'a>,
        accounts: &crate::instruction::transaction_step::Accounts<'a>,
        caller: Address,
        trx: &Transaction,
    ) -> Result<Self, ProgramError> {
        let owner = match crate::account::tag(program_id, info)? {
            Holder::TAG => {
                let holder = Holder::from_account(program_id, info)?;
                holder.owner
            }
            FinalizedState::TAG => {
                let finalized_storage = FinalizedState::from_account(program_id, info)?;
                if !finalized_storage.is_outdated(&trx.hash()) {
                    return Err!(Error::StorageAccountFinalized.into(); "Transaction already finalized");
                }

                finalized_storage.owner
            }
            _ => {
                return Err!(ProgramError::InvalidAccountData; "Account {} - expected finalized storage or holder", info.key)
            }
        };

        if &owner != accounts.operator.key {
            return Err!(ProgramError::InvalidAccountData; "Account {} - invalid state account owner", info.key);
        }

        let data = crate::account::state::Data {
            owner,
            transaction_hash: trx.hash(),
            caller,
            gas_limit: trx.gas_limit(),
            gas_price: trx.gas_price(),
            gas_used: U256::ZERO,
            operator: *accounts.operator.key,
            slot: Clock::get()?.slot,
            accounts_len: accounts.remaining_accounts.len(),
            evm_state_len: 0,
            evm_machine_len: 0,
        };

        info.data.borrow_mut()[0] = 0_u8;
        let mut storage = State::init(program_id, info, data)?;

        storage.write_blocked_accounts(program_id, accounts.remaining_accounts)?;
        Ok(storage)
    }

    pub fn restore(
        program_id: &Pubkey,
        info: &'a AccountInfo<'a>,
        operator: &Operator,
        remaining_accounts: &[AccountInfo],
        is_cancelling: bool,
    ) -> Result<(Self, BlockedAccounts), ProgramError> {
        let account_tag = crate::account::tag(program_id, info)?;
        if account_tag == FinalizedState::TAG {
            return Err!(Error::StorageAccountFinalized.into(); "Account {} - Storage Finalized", info.key);
        }
        if account_tag == Holder::TAG {
            return Err!(Error::StorageAccountUninitialized.into(); "Account {} - Storage Uninitialized", info.key);
        }

        let mut storage = State::from_account(program_id, info)?;
        let blocked_accounts =
            storage.check_blocked_accounts(program_id, remaining_accounts, is_cancelling)?;

        storage.operator = *operator.key;
        Ok((storage, blocked_accounts))
    }

    pub fn finalize(self) -> Result<FinalizedState<'a>, ProgramError> {
        debug_print!("Finalize Storage {}", self.info.key);

        let finalized_data = crate::account::state::FinalizedData {
            owner: self.owner,
            transaction_hash: self.transaction_hash,
        };

        let finalized = unsafe { self.replace(finalized_data) }?;
        Ok(finalized)
    }

    pub fn read_blocked_accounts(&self) -> Result<BlockedAccounts, ProgramError> {
        let (begin, end) = self.blocked_accounts_region();

        let account_data = self.info.try_borrow_data()?;
        if account_data.len() < end {
            return Err!(ProgramError::AccountDataTooSmall; "Account {} - data too small, required: {}", self.info.key, end);
        }

        let keys_storage = &account_data[begin..end];
        let chunks = keys_storage.chunks_exact(ACCOUNT_CHUNK_LEN);
        let accounts = chunks
            .map(|c| c.split_at(2))
            .map(|(meta, key)| BlockedAccountMeta {
                key: Pubkey::try_from(key).expect("key is 32 bytes"),
                exists: meta[1] != 0,
                is_writable: meta[0] != 0,
            })
            .collect();

        Ok(accounts)
    }

    #[cfg(target_os = "solana")]
    fn write_blocked_accounts(
        &mut self,
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> Result<(), ProgramError> {
        assert_eq!(self.accounts_len, accounts.len()); // should be always true

        let (begin, end) = self.blocked_accounts_region();

        let mut account_data = self.info.try_borrow_mut_data()?;
        if account_data.len() < end {
            return Err!(ProgramError::AccountDataTooSmall; "Account {} - data too small, required: {}", self.info.key, end);
        }

        let accounts_storage = &mut account_data[begin..end];
        let accounts_storage = accounts_storage.chunks_exact_mut(ACCOUNT_CHUNK_LEN);
        for (info, account_storage) in accounts.iter().zip(accounts_storage) {
            account_storage[0] = u8::from(info.is_writable);
            account_storage[1] = u8::from(Self::account_exists(program_id, info));
            account_storage[2..].copy_from_slice(info.key.as_ref());
        }

        Ok(())
    }

    pub fn update_blocked_accounts<I>(&mut self, accounts: I) -> Result<(), Error>
    where
        I: ExactSizeIterator<Item = BlockedAccountMeta>,
    {
        let evm_data_len = self.evm_state_len + self.evm_machine_len;
        let (evm_data_offset, _) = self.evm_data_region();
        let evm_data_range = evm_data_offset..evm_data_offset + evm_data_len;

        self.accounts_len = accounts.len();
        let (accounts_begin, accounts_end) = self.blocked_accounts_region();

        let mut data = self.info.try_borrow_mut_data()?;
        // Move EVM data
        data.copy_within(evm_data_range, accounts_end);

        // Write accounts
        let accounts_storage = &mut data[accounts_begin..accounts_end];
        let accounts_storage = accounts_storage.chunks_exact_mut(ACCOUNT_CHUNK_LEN);
        for (meta, account_storage) in accounts.zip(accounts_storage) {
            account_storage[0] = u8::from(meta.is_writable);
            account_storage[1] = u8::from(meta.exists);
            account_storage[2..].copy_from_slice(meta.key.as_ref());
        }

        Ok(())
    }

    fn check_blocked_accounts(
        &self,
        program_id: &Pubkey,
        remaining_accounts: &[AccountInfo],
        is_cancelling: bool,
    ) -> Result<BlockedAccounts, ProgramError> {
        let blocked_accounts = self.read_blocked_accounts()?;
        if blocked_accounts.len() != remaining_accounts.len() {
            return Err!(ProgramError::NotEnoughAccountKeys; "Invalid number of accounts");
        }

        for (blocked, info) in blocked_accounts.iter().zip(remaining_accounts) {
            if blocked.key != *info.key {
                return Err!(ProgramError::InvalidAccountData; "Expected account {}, found {}", blocked.key, info.key);
            }

            if blocked.is_writable && !info.is_writable {
                return Err!(ProgramError::InvalidAccountData; "Expected account {} is_writable: {}", info.key, blocked.is_writable);
            }

            if !is_cancelling && !blocked.exists && Self::account_exists(program_id, info) {
                return Err!(
                    ProgramError::AccountAlreadyInitialized;
                    "Blocked nonexistent account {} was created/initialized outside current transaction. \
                    Transaction is being cancelled in order to prevent possible data corruption.",
                    info.key
                );
            }
        }

        Ok(blocked_accounts)
    }

    #[must_use]
    pub fn evm_data(&self) -> Ref<[u8]> {
        let (begin, end) = self.evm_data_region();

        let data = self.info.data.borrow();
        Ref::map(data, |d| &d[begin..end])
    }

    #[must_use]
    pub fn evm_data_mut(&mut self) -> RefMut<[u8]> {
        let (begin, end) = self.evm_data_region();

        let data = self.info.data.borrow_mut();
        RefMut::map(data, |d| &mut d[begin..end])
    }

    #[must_use]
    fn evm_data_region(&self) -> (usize, usize) {
        let (_, accounts_region_end) = self.blocked_accounts_region();

        let begin = accounts_region_end;
        let end = self.info.data_len();

        (begin, end)
    }

    #[must_use]
    fn blocked_accounts_region(&self) -> (usize, usize) {
        let begin = Self::SIZE;
        let end = begin + self.accounts_len * ACCOUNT_CHUNK_LEN;

        (begin, end)
    }

    #[must_use]
    fn account_exists(program_id: &Pubkey, info: &AccountInfo) -> bool {
        (info.owner == program_id)
            && !info.data_is_empty()
            && (info.data.borrow()[0] == EthereumAccount::TAG)
    }
}
