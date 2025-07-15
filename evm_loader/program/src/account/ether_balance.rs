use std::cell::{Ref, RefMut};
use std::mem::size_of;

use crate::account::TAG_EMPTY;
use crate::{
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

use super::{Account, AccountDispatch, AccountHeader, ACCOUNT_PREFIX_LEN, TAG_ACCOUNT_BALANCE};

#[repr(C, packed)]
pub struct HeaderV0 {
    pub address: Address,
    pub chain_id: u64,
    pub trx_count: u64,
    pub balance: U256,
}
impl AccountHeader for HeaderV0 {
    const VERSION: u8 = 0;
}

#[repr(C, packed)]
pub struct HeaderWithRevision {
    pub v0: HeaderV0,
    pub revision: u32,
}

impl AccountHeader for HeaderWithRevision {
    const VERSION: u8 = 2;
}

#[repr(C, packed)]
pub struct HeaderWithSolanaAddress {
    pub v2: HeaderWithRevision,
    pub solana_address: Pubkey,
}

impl AccountHeader for HeaderWithSolanaAddress {
    const VERSION: u8 = 3;
}

// Set the last version of the Header struct here
// and change the `header_size` and `header_upgrade` functions
pub type Header = HeaderWithSolanaAddress;

pub struct BalanceAccount<'a> {
    pub account: Account<'a>, // TODO: make it private after emulator changes
}

impl<'a> BalanceAccount<'a> {
    #[must_use]
    pub fn required_account_size(is_solana_user: bool) -> usize {
        let header_size = if is_solana_user {
            size_of::<HeaderWithSolanaAddress>()
        } else {
            size_of::<HeaderWithRevision>()
        };

        ACCOUNT_PREFIX_LEN + header_size
    }

    #[must_use]
    pub fn required_header_realloc(&self) -> usize {
        let allocated_header_size = self.header_size();
        size_of::<Header>().saturating_sub(allocated_header_size)
    }

    pub fn from_account_info(program_id: Pubkey, account: &AccountInfo<'a>) -> Result<Self> {
        let account = account.clone().into();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, account: Account<'a>) -> Result<Self> {
        account.validate_tag(program_id, TAG_ACCOUNT_BALANCE)?;

        Ok(Self { account })
    }

    /// # Safety
    /// It's a caller responsibility to validate the account tag
    #[must_use]
    pub unsafe fn from_account_unchecked(account: Account<'a>) -> Self {
        Self { account }
    }

    pub fn initialize(
        mut account: Account<'a>,
        program_id: Pubkey,
        address: Address,
        chain_id: u64,
    ) -> Result<Self> {
        assert_eq!(account.data_len(), Self::required_account_size(false));
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_ACCOUNT_BALANCE, HeaderWithRevision::VERSION)?;
        {
            let mut header: RefMut<HeaderV0> = account.header_mut();
            header.address = address;
            header.chain_id = chain_id;
            header.trx_count = 0;
            header.balance = U256::ZERO;
        }
        {
            let mut header: RefMut<HeaderWithRevision> = account.header_mut();
            header.revision = 1;
        }

        Ok(Self { account })
    }

    pub fn initialize_for_solana_user(
        mut account: Account<'a>,
        program_id: Pubkey,
        pubkey: Pubkey,
        chain_id: u64,
    ) -> Result<Self> {
        assert_eq!(account.data_len(), Self::required_account_size(true));
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_ACCOUNT_BALANCE, HeaderWithSolanaAddress::VERSION)?;

        {
            let mut header: RefMut<HeaderV0> = account.header_mut();
            header.address = Address::from_solana_address(&pubkey);
            header.chain_id = chain_id;
            header.trx_count = 0;
            header.balance = U256::ZERO;
        }
        {
            let mut header: RefMut<HeaderWithRevision> = account.header_mut();
            header.revision = 1;
        }
        {
            let mut header: RefMut<HeaderWithSolanaAddress> = account.header_mut();
            header.solana_address = pubkey;
        }

        Ok(Self { account })
    }

    fn header_size(&self) -> usize {
        match self.account.header_version() {
            0 | 1 => size_of::<HeaderV0>(),
            HeaderWithRevision::VERSION => size_of::<HeaderWithRevision>(),
            HeaderWithSolanaAddress::VERSION => size_of::<HeaderWithSolanaAddress>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    fn header_upgrade(&mut self) -> Result<()> {
        match self.account.header_version() {
            0 | 1 => {
                self.account.expand_header::<HeaderV0, Header>()?;
            }
            HeaderWithRevision::VERSION => {
                self.account.expand_header::<HeaderWithRevision, Header>()?;
            }
            HeaderWithSolanaAddress::VERSION => {
                self.account
                    .expand_header::<HeaderWithSolanaAddress, Header>()?;
            }
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }

        Ok(())
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    #[must_use]
    pub fn address(&self) -> Address {
        let header: Ref<HeaderV0> = self.account.header();
        header.address
    }

    #[must_use]
    pub fn solana_address(&self) -> Option<Pubkey> {
        if self.account.header_version() < HeaderWithSolanaAddress::VERSION {
            return None;
        }

        let header: Ref<HeaderWithSolanaAddress> = self.account.header();

        if header.solana_address == Pubkey::default() {
            return None;
        }

        Some(header.solana_address)
    }

    pub fn set_solana_address(&mut self, pubkey: Pubkey) -> Result<()> {
        if self.account.header_version() < HeaderWithSolanaAddress::VERSION {
            self.header_upgrade()?;
        }

        {
            let mut header: RefMut<HeaderWithSolanaAddress> = self.account.header_mut();
            header.solana_address = pubkey;
        }
        self.increment_revision()
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        let header: Ref<HeaderV0> = self.account.header();
        header.chain_id
    }

    #[must_use]
    pub fn nonce(&self) -> u64 {
        let header: Ref<HeaderV0> = self.account.header();
        header.trx_count
    }

    pub fn override_nonce_by(&mut self, value: u64) {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();
        header.trx_count = value;
    }

    pub fn override_balance_by(&mut self, value: U256) {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();
        header.balance = value;
    }

    #[must_use]
    pub fn exists(&self) -> bool {
        let header: Ref<HeaderV0> = self.account.header();

        ({ header.trx_count } > 0) || ({ header.balance } > 0)
    }

    pub fn increment_nonce(&mut self) -> Result<()> {
        self.increment_nonce_by(1)
    }

    pub fn increment_nonce_by(&mut self, value: u64) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

        header.trx_count = header
            .trx_count
            .checked_add(value)
            .ok_or_else(|| Error::NonceOverflow(header.address))?;

        Ok(())
    }

    #[must_use]
    pub fn balance(&self) -> U256 {
        let header: Ref<HeaderV0> = self.account.header();
        header.balance
    }

    /// # Safety
    ///  It's the caller's responsibility to increment the revision
    pub unsafe fn transfer_without_revision(
        &mut self,
        target: &mut BalanceAccount,
        value: U256,
    ) -> Result<()> {
        if self.account.pubkey() == target.account.pubkey() {
            return Ok(());
        }

        assert_eq!(self.chain_id(), target.chain_id());

        self.burn_without_revision(value)?;
        target.mint_without_revision(value)
    }

    /// # Safety
    ///  It's the caller's responsibility to increment the revision
    pub unsafe fn burn_without_revision(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

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

    /// # Safety
    ///  It's the caller's responsibility to increment the revision
    pub unsafe fn mint_without_revision(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }

    pub fn transfer(&mut self, target: &mut BalanceAccount, value: U256) -> Result<()> {
        unsafe { self.transfer_without_revision(target, value) }?;

        self.increment_revision()?;
        target.increment_revision()
    }

    pub fn burn(&mut self, value: U256) -> Result<()> {
        unsafe { self.burn_without_revision(value) }?;

        self.increment_revision()
    }

    pub fn mint(&mut self, value: U256) -> Result<()> {
        unsafe { self.mint_without_revision(value) }?;

        self.increment_revision()
    }

    pub fn burn_gas(&mut self, gas: U256, gas_price: U256) -> Result<()> {
        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };

        if tokens == U256::ZERO {
            return Ok(());
        }

        self.burn(tokens)
    }

    pub fn refund_gas(&mut self, gas: U256, gas_price: U256) -> Result<()> {
        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };

        if tokens == U256::ZERO {
            return Ok(());
        }

        self.mint(tokens)
    }

    #[must_use]
    pub fn revision(&self) -> u32 {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            return 0;
        }

        let header: Ref<HeaderWithRevision> = self.account.header();
        header.revision
    }

    pub fn increment_revision(&mut self) -> Result<()> {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            self.header_upgrade()?;
        }

        let mut header: RefMut<HeaderWithRevision> = self.account.header_mut();
        header.revision = header.revision.wrapping_add(1);

        Ok(())
    }
}
