use std::cell::{Ref, RefMut};
use std::mem::size_of;

use crate::config::{GAS_LIMIT_MULTIPLIER_NO_CHAINID, OPERATOR_PRIORITY_SLOTS, PAYMENT_TO_DEPOSIT};
use crate::error::{Error, Result};
use crate::types::{Address, Transaction};
use ethnum::U256;
use solana_program::clock::Clock;
use solana_program::program_error::ProgramError;
use solana_program::sysvar::Sysvar;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use super::{
    AccountsDB, Incinerator, Operator, ACCOUNT_PREFIX_LEN, TAG_EMPTY, TAG_HOLDER, TAG_STATE,
    TAG_STATE_FINALIZED,
};

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub transaction_hash: [u8; 32],
    /// Ethereum transaction caller address
    pub origin: Address,
    /// Ethereum transaction chain_id
    pub chain_id: u64,
    /// Ethereum transaction gas limit
    pub gas_limit: U256,
    /// Ethereum transaction gas price
    pub gas_price: U256,
    /// Ethereum transaction gas used and paid
    pub gas_used: U256,
    /// Operator public key
    pub operator: Pubkey,
    /// Starting slot for this operator
    pub slot: u64,
    /// Stored accounts length
    pub accounts_len: usize,
    /// Stored EVM State length
    pub evm_state_len: usize,
    /// Stored EVM Machine length
    pub evm_machine_len: usize,
}

#[repr(C, packed)]
pub struct BlockedAccount {
    pub is_writable: bool,
    pub blocked: bool,
    pub key: Pubkey,
}

pub struct StateAccount<'a> {
    account: AccountInfo<'a>,
}

const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;
const BLOCKED_ACCOUNTS_OFFSET: usize = HEADER_OFFSET + size_of::<Header>();

impl<'a> StateAccount<'a> {
    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, &account, TAG_STATE)?;

        Ok(Self { account })
    }

    pub fn new(
        program_id: &Pubkey,
        info: AccountInfo<'a>,
        accounts: &AccountsDB<'a>,
        origin: Address,
        trx: &Transaction,
    ) -> Result<Self> {
        let tag = super::tag(program_id, &info)?;
        if matches!(tag, TAG_HOLDER | TAG_STATE_FINALIZED) {
            super::set_tag(program_id, &info, TAG_STATE)?;
        }

        let mut state = Self::from_account(program_id, info)?;
        state.validate_owner(accounts.operator())?;

        if (tag == TAG_STATE_FINALIZED) && (state.trx_hash() == trx.hash) {
            return Err(Error::StorageAccountFinalized);
        }

        // Set header
        {
            let mut header = state.header_mut();
            header.transaction_hash = trx.hash();
            header.origin = origin;
            header.chain_id = trx.chain_id().unwrap_or(crate::config::DEFAULT_CHAIN_ID);
            header.gas_limit = trx.gas_limit();
            header.gas_price = trx.gas_price();
            header.gas_used = U256::ZERO;
            header.operator = accounts.operator_key();
            header.slot = Clock::get()?.slot;
            header.accounts_len = accounts.accounts_len();
            header.evm_machine_len = 0;
            header.evm_state_len = 0;
        }
        // Block accounts
        for (block, account) in state.blocked_accounts_mut().iter_mut().zip(accounts) {
            block.is_writable = account.is_writable;
            block.key = *account.key;
            if account.owner == program_id {
                super::block(program_id, account)?;
                block.blocked = true;
            } else {
                block.blocked = false;
            }
        }

        // Make deposit
        let system = accounts.system();
        system.transfer(accounts.operator(), &state.account, PAYMENT_TO_DEPOSIT)?;

        Ok(state)
    }

    pub fn restore(
        program_id: &Pubkey,
        info: AccountInfo<'a>,
        accounts: &AccountsDB,
        is_canceling: bool,
    ) -> Result<Self> {
        let mut state = Self::from_account(program_id, info)?;

        if state.blocked_accounts_len() != accounts.accounts_len() {
            return Err(ProgramError::NotEnoughAccountKeys.into());
        }

        // Check blocked accounts
        for (block, account) in state.blocked_accounts().iter().zip(accounts) {
            if &block.key != account.key {
                return Err(Error::AccountInvalidKey(*account.key, block.key));
            }

            if block.is_writable && !account.is_writable {
                return Err(Error::AccountNotWritable(*account.key));
            }

            if !is_canceling && (account.owner == program_id) && !block.blocked {
                if super::is_blocked(program_id, account)? {
                    return Err(Error::AccountCreatedByAnotherTransaction(*account.key));
                }

                super::validate_tag(program_id, account, TAG_EMPTY)
                    .map_err(|_| Error::AccountCreatedByAnotherTransaction(*account.key))?;
            }
        }

        state.update_priority_operator(&accounts.operator)?;

        Ok(state)
    }

    fn finalize_impl(
        self,
        program_id: &Pubkey,
        accounts: &AccountsDB,
        target: &AccountInfo<'a>,
    ) -> Result<()> {
        debug_print!("Finalize Storage {}", self.account.key);

        // Return deposit
        **self.account.lamports.borrow_mut() -= PAYMENT_TO_DEPOSIT;
        **target.lamports.borrow_mut() += PAYMENT_TO_DEPOSIT;

        // Unblock accounts
        for (block, account) in self.blocked_accounts().iter().zip(accounts) {
            if &block.key != account.key {
                return Err(Error::AccountInvalidKey(*account.key, block.key));
            }

            if !block.blocked {
                continue;
            }

            super::unblock(program_id, account)?;
        }

        // Change tag to finalized
        let account = self.account.clone();
        std::mem::drop(self);

        super::set_tag(account.owner, &account, TAG_STATE_FINALIZED)
    }

    pub fn finalize(
        self,
        program_id: &Pubkey,
        accounts: &AccountsDB,
        operator: &Operator<'a>,
    ) -> Result<()> {
        self.finalize_impl(program_id, accounts, operator)
    }

    pub fn cancel(
        self,
        program_id: &Pubkey,
        accounts: &AccountsDB,
        incinerator: &Incinerator<'a>,
    ) -> Result<()> {
        self.finalize_impl(program_id, accounts, incinerator)
    }

    #[inline]
    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn blocked_accounts_len(&self) -> usize {
        self.header().accounts_len
    }

    #[inline]
    pub fn blocked_accounts(&self) -> Ref<[BlockedAccount]> {
        let accounts_len = self.blocked_accounts_len();
        let accounts_len_bytes = accounts_len * size_of::<BlockedAccount>();

        let data = self.account.data.borrow();
        Ref::map(data, |d| {
            let bytes = &d[BLOCKED_ACCOUNTS_OFFSET..][..accounts_len_bytes];

            unsafe {
                let ptr = bytes.as_ptr().cast();
                std::slice::from_raw_parts(ptr, accounts_len)
            }
        })
    }

    #[inline]
    fn blocked_accounts_mut(&mut self) -> RefMut<[BlockedAccount]> {
        let accounts_len = self.blocked_accounts_len();
        let accounts_len_bytes = accounts_len * size_of::<BlockedAccount>();

        let data = self.account.data.borrow_mut();
        RefMut::map(data, |d| {
            let bytes: &mut [u8] = &mut d[BLOCKED_ACCOUNTS_OFFSET..][..accounts_len_bytes];

            unsafe {
                let ptr = bytes.as_mut_ptr().cast();
                std::slice::from_raw_parts_mut(ptr, accounts_len)
            }
        })
    }

    pub fn buffer(&self) -> Ref<[u8]> {
        let accounts_len_bytes = self.blocked_accounts_len() * size_of::<BlockedAccount>();
        let buffer_offset = BLOCKED_ACCOUNTS_OFFSET + accounts_len_bytes;

        let data = self.account.data.borrow();
        Ref::map(data, |d| &d[buffer_offset..])
    }

    pub fn buffer_mut(&mut self) -> RefMut<[u8]> {
        let accounts_len_bytes = self.blocked_accounts_len() * size_of::<BlockedAccount>();
        let buffer_offset = BLOCKED_ACCOUNTS_OFFSET + accounts_len_bytes;

        let data = self.account.data.borrow_mut();
        RefMut::map(data, |d| &mut d[buffer_offset..])
    }

    pub fn buffer_variables(&self) -> (usize, usize) {
        let header = self.header();
        (header.evm_state_len, header.evm_machine_len)
    }

    pub fn set_buffer_variables(&mut self, evm_state_len: usize, evm_machine_len: usize) {
        let mut header = self.header_mut();
        header.evm_state_len = evm_state_len;
        header.evm_machine_len = evm_machine_len;
    }

    pub fn owner(&self) -> Pubkey {
        self.header().owner
    }

    fn validate_owner(&self, operator: &Operator) -> Result<()> {
        let owner = self.owner();
        let operator = *operator.key;

        if owner != operator {
            return Err(Error::HolderInvalidOwner(owner, operator));
        }

        Ok(())
    }

    fn update_priority_operator(&mut self, operator: &Operator) -> Result<()> {
        let mut header = self.header_mut();

        if operator.key != &header.operator {
            let clock = Clock::get()?;
            if (clock.slot - header.slot) <= OPERATOR_PRIORITY_SLOTS {
                return Err(Error::HolderInvalidOwner(header.owner, *operator.key));
            }

            header.operator = *operator.key;
            header.slot = clock.slot;
        }

        Ok(())
    }

    pub fn trx_hash(&self) -> [u8; 32] {
        self.header().transaction_hash
    }

    pub fn trx_origin(&self) -> Address {
        self.header().origin
    }

    pub fn trx_chain_id(&self) -> u64 {
        self.header().chain_id
    }

    pub fn trx_gas_price(&self) -> U256 {
        self.header().gas_price
    }

    pub fn trx_gas_limit(&self) -> U256 {
        self.header().gas_limit
    }

    pub fn gas_used(&self) -> U256 {
        self.header().gas_used
    }

    pub fn burn_gas(&mut self, amount: U256) -> Result<()> {
        let mut header = self.header_mut();

        let total_gas_used = header.gas_used.saturating_add(amount);
        let gas_limit = header.gas_limit;

        if total_gas_used > gas_limit {
            return Err(Error::OutOfGas(gas_limit, total_gas_used));
        }

        header.gas_used = total_gas_used;

        Ok(())
    }

    pub fn use_gas_limit_multiplier(&mut self) {
        let mut header = self.header_mut();

        let gas_multiplier = U256::from(GAS_LIMIT_MULTIPLIER_NO_CHAINID);
        header.gas_limit = header.gas_limit.saturating_mul(gas_multiplier);
    }
}
