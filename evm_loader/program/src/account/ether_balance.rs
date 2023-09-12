use std::{
    cell::{Ref, RefMut},
    mem::size_of,
};

use crate::{
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, system_program};

use super::{AccountsDB, ACCOUNT_PREFIX_LEN, ACCOUNT_SEED_VERSION, TAG_ACCOUNT_BALANCE, TAG_EMPTY};

#[repr(C, packed)]
pub struct Header {
    // address: Address,
    pub chain_id: u64,
    pub trx_count: u64,
    pub balance: U256,
}

pub struct BalanceAccount<'a> {
    address: Option<Address>,
    account: AccountInfo<'a>,
}

const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;

impl<'a> BalanceAccount<'a> {
    pub fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>()
    }

    pub fn from_account(
        program_id: &Pubkey,
        account: AccountInfo<'a>,
        address: Option<Address>,
    ) -> Result<Self> {
        super::validate_tag(program_id, &account, TAG_ACCOUNT_BALANCE)?;

        Ok(Self { address, account })
    }

    pub fn create(address: Address, chain_id: u64, accounts: &AccountsDB<'a>) -> Result<Self> {
        let (pubkey, bump_seed) = address.find_balance_address(&crate::ID, chain_id);

        let account = accounts.get(&pubkey).clone();

        if system_program::check_id(account.owner) {
            let chain_id = U256::from(chain_id);

            let program_seeds: &[&[u8]] = &[
                &[ACCOUNT_SEED_VERSION],
                address.as_bytes(),
                &chain_id.to_be_bytes(),
                &[bump_seed],
            ];

            let system = accounts.system();
            let operator = accounts.operator();

            system.create_pda_account(
                &crate::ID,
                operator,
                &account,
                program_seeds,
                ACCOUNT_PREFIX_LEN + size_of::<Header>(),
            )?;
        }

        match super::tag(&crate::ID, &account)? {
            TAG_ACCOUNT_BALANCE => {
                let balance_account = Self::from_account(&crate::ID, account, Some(address))?;
                if balance_account.chain_id() != chain_id {
                    return Err(Error::AccountInvalidData(pubkey));
                }

                Ok(balance_account)
            }
            TAG_EMPTY => {
                super::set_tag(&crate::ID, &account, TAG_ACCOUNT_BALANCE)?;
                let mut balance_account = Self::from_account(&crate::ID, account, Some(address))?;
                {
                    let mut header = balance_account.header_mut();
                    header.chain_id = chain_id;
                    header.trx_count = 0;
                    header.balance = U256::ZERO;
                }

                Ok(balance_account)
            }
            _ => Err(Error::AccountInvalidTag(pubkey, TAG_ACCOUNT_BALANCE)),
        }
    }

    #[inline]
    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    pub fn chain_id(&self) -> u64 {
        self.header().chain_id
    }

    pub fn nonce(&self) -> u64 {
        self.header().trx_count
    }

    pub fn exists(&self) -> bool {
        let header = self.header();

        ({ header.trx_count } > 0) || ({ header.balance } > 0)
    }

    pub fn increment_nonce(&mut self) -> Result<()> {
        let address = self.address.unwrap_or_default();

        let mut header = self.header_mut();
        if header.trx_count == u64::MAX {
            return Err(Error::NonceOverflow(address));
        }

        header.trx_count += 1;

        Ok(())
    }

    pub fn balance(&self) -> U256 {
        self.header().balance
    }

    pub fn transfer(&mut self, target: &mut BalanceAccount, value: U256) -> Result<()> {
        if self.account.key == target.account.key {
            return Ok(());
        }

        assert_eq!(self.chain_id(), target.chain_id());

        self.burn(value)?;
        target.mint(value)
    }

    pub fn burn(&mut self, value: U256) -> Result<()> {
        let address = self.address.unwrap_or_default();

        let mut header = self.header_mut();

        header.balance = header
            .balance
            .checked_sub(value)
            .ok_or(Error::InsufficientBalance(address, header.chain_id, value))?;

        Ok(())
    }

    pub fn mint(&mut self, value: U256) -> Result<()> {
        let mut header = self.header_mut();

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }
}
