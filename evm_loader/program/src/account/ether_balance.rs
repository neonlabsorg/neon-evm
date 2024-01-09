use std::{
    cell::{Ref, RefMut},
    mem::size_of,
};

use crate::{
    account::{TAG_ACCOUNT_CONTRACT, TAG_EMPTY},
    account_storage::KeysCache,
    config::DEFAULT_CHAIN_ID,
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, system_program};

use super::{AccountsDB, ACCOUNT_PREFIX_LEN, ACCOUNT_SEED_VERSION, TAG_ACCOUNT_BALANCE};

#[repr(C, packed)]
struct Header {
    pub address: Address,
    chain_id: u64,
    trx_count: u64,
    balance: U256,
}

pub struct BalanceAccount<'a> {
    account: AccountInfo<'a>,
}

const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;

impl<'a> BalanceAccount<'a> {
    #[must_use]
    pub fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>()
    }

    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, &account, TAG_ACCOUNT_BALANCE)?;

        Ok(Self { account })
    }

    pub fn create(
        address: Address,
        chain_id: u64,
        accounts: &AccountsDB<'a>,
        keys: Option<&KeysCache>,
    ) -> Result<Self> {
        let (pubkey, bump_seed) = keys.map_or_else(
            || address.find_balance_address(&crate::ID, chain_id),
            |keys| keys.balance_with_bump_seed(&crate::ID, address, chain_id),
        );

        // Already created. Return immediately
        let account = accounts.get(&pubkey).clone();
        if !system_program::check_id(account.owner) {
            let balance_account = Self::from_account(&crate::ID, account)?;
            assert_eq!(balance_account.address(), address);
            assert_eq!(balance_account.chain_id(), chain_id);

            return Ok(balance_account);
        }

        if chain_id == DEFAULT_CHAIN_ID {
            // Make sure no legacy account exists
            let legacy_pubkey = keys.map_or_else(
                || address.find_solana_address(&crate::ID).0,
                |keys| keys.contract(&crate::ID, address),
            );

            let legacy_account = accounts.get(&legacy_pubkey);
            if crate::check_id(legacy_account.owner) {
                let legacy_tag = super::tag(&crate::ID, legacy_account)?;
                assert!(legacy_tag == TAG_EMPTY || legacy_tag == TAG_ACCOUNT_CONTRACT);
            }
        }

        // Create a new account
        let program_seeds: &[&[u8]] = &[
            &[ACCOUNT_SEED_VERSION],
            address.as_bytes(),
            &U256::from(chain_id).to_be_bytes(),
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

        Self::new(&crate::ID, address, account, chain_id, 0, U256::ZERO)
    }

    pub fn new(
        program_id: &Pubkey,
        address: Address,
        account: AccountInfo<'a>,
        chain_id: u64,
        trx_count: u64,
        balance: U256,
    ) -> Result<Self> {
        super::set_tag(program_id, &account, TAG_ACCOUNT_BALANCE)?;

        let mut balance_account = Self { account };

        {
            let mut header = balance_account.header_mut();
            header.address = address;
            header.chain_id = chain_id;
            header.trx_count = trx_count;
            header.balance = balance;
        }

        Ok(balance_account)
    }

    #[must_use]
    pub fn pubkey(&self) -> &'a Pubkey {
        self.account.key
    }

    #[inline]
    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    #[must_use]
    pub fn address(&self) -> Address {
        self.header().address
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        self.header().chain_id
    }

    #[must_use]
    pub fn nonce(&self) -> u64 {
        self.header().trx_count
    }

    #[must_use]
    pub fn exists(&self) -> bool {
        let header = self.header();

        ({ header.trx_count } > 0) || ({ header.balance } > 0)
    }

    pub fn increment_nonce(&mut self) -> Result<()> {
        self.increment_nonce_by(1)
    }

    pub fn increment_nonce_by(&mut self, value: u64) -> Result<()> {
        let mut header = self.header_mut();

        header.trx_count = header
            .trx_count
            .checked_add(value)
            .ok_or_else(|| Error::NonceOverflow(header.address))?;

        Ok(())
    }

    #[must_use]
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
        let mut header = self.header_mut();

        header.balance = header
            .balance
            .checked_sub(value)
            .ok_or(Error::InsufficientBalance(
                header.address,
                header.chain_id,
                value,
            ))?;

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
