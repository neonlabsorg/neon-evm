use std::cell::{Ref, RefMut};
use std::mem::size_of;

use crate::{
    account_storage::KeysCache,
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::{pubkey::Pubkey, rent::Rent, system_program};

use super::{
    Account, AccountDispatch, AccountHeader, AccountsDB, ACCOUNT_PREFIX_LEN, ACCOUNT_SEED_VERSION,
    TAG_ACCOUNT_BALANCE, TAG_EMPTY,
};

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
    pub fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>()
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

    pub fn create_for_solana_user(
        pubkey: Pubkey,
        chain_id: u64,
        accounts: &AccountsDB<'a>,
        rent: &Rent,
    ) -> Result<Self> {
        let address = Address::from_solana_address(&pubkey);

        let mut balance = Self::create(address, chain_id, accounts, None, rent)?;
        if let Some(solana_address) = balance.solana_address() {
            assert_eq!(solana_address, pubkey);
        } else {
            assert_eq!(balance.nonce(), 0);

            let mut header: RefMut<Header> = balance.account.header_mut();
            header.solana_address = pubkey;
        }

        Ok(balance)
    }

    pub fn create(
        address: Address,
        chain_id: u64,
        accounts: &AccountsDB<'a>,
        keys: Option<&KeysCache>,
        rent: &Rent,
    ) -> Result<Self> {
        let (pubkey, bump_seed) = keys.map_or_else(
            || address.find_balance_address(&crate::ID, chain_id),
            |keys| keys.balance_with_bump_seed(&crate::ID, address, chain_id),
        );

        // Already created. Return immidiately
        let account = accounts.get(&pubkey).clone();
        if !system_program::check_id(account.owner) {
            let balance_account = Self::from_account(crate::ID, account.into())?;
            assert_eq!(balance_account.address(), address);
            assert_eq!(balance_account.chain_id(), chain_id);

            return Ok(balance_account);
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
            rent,
        )?;

        Self::initialize(account.into(), crate::ID, address, chain_id)
    }

    pub fn initialize(
        mut account: Account<'a>,
        program_id: Pubkey,
        address: Address,
        chain_id: u64,
    ) -> Result<Self> {
        assert_eq!(account.data_len(), Self::required_account_size());
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        // TODO: After AccountStorage fix do not allocate space for pubkey for non solana users
        account.init_tag(TAG_ACCOUNT_BALANCE, Header::VERSION)?;
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
        assert_eq!(account.data_len(), Self::required_account_size());
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_ACCOUNT_BALANCE, Header::VERSION)?;

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

    fn header_upgrade(&mut self, rent: &Rent, db: &AccountsDB<'a>) -> Result<()> {
        match self.account.header_version() {
            0 | 1 => {
                self.account.expand_header::<HeaderV0, Header>(rent, db)?;
            }
            HeaderWithRevision::VERSION => {
                self.account
                    .expand_header::<HeaderWithRevision, Header>(rent, db)?;
            }
            HeaderWithSolanaAddress::VERSION => {
                self.account
                    .expand_header::<HeaderWithSolanaAddress, Header>(rent, db)?;
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

    pub fn transfer(&mut self, target: &mut BalanceAccount, value: U256) -> Result<()> {
        if self.account.pubkey() == target.account.pubkey() {
            return Ok(());
        }

        assert_eq!(self.chain_id(), target.chain_id());

        self.burn(value)?;
        target.mint(value)
    }

    pub fn burn(&mut self, value: U256) -> Result<()> {
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

    pub fn mint(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }

    #[must_use]
    pub fn revision(&self) -> u32 {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            return 0;
        }

        let header: Ref<HeaderWithRevision> = self.account.header();
        header.revision
    }

    pub fn increment_revision(&mut self, rent: &Rent, db: &AccountsDB<'a>) -> Result<()> {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            self.header_upgrade(rent, db)?;
        }

        let mut header: RefMut<HeaderWithRevision> = self.account.header_mut();
        header.revision = header.revision.wrapping_add(1);

        Ok(())
    }
}
