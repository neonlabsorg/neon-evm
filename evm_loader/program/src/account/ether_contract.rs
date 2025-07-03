use crate::{
    account::TAG_EMPTY,
    error::{Error, Result},
    types::Address,
};
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::MAX_PERMITTED_DATA_INCREASE,
    pubkey::Pubkey,
};
use std::{
    cell::{Ref, RefMut},
    mem::size_of,
};

use crate::config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;

use super::{
    Account, AccountDispatch, AccountHeader, ZeroInit, ACCOUNT_PREFIX_LEN, TAG_ACCOUNT_CONTRACT,
};

#[derive(Eq, PartialEq)]
pub enum AllocateResult {
    Ready,
    NeedMore,
}

#[repr(C, packed)]
pub struct HeaderV0 {
    pub address: Address,
    pub chain_id: u64,
    pub generation: u32,
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
pub struct HeaderWithTimestamp {
    pub v2: HeaderWithRevision,
    pub timestamp_used_at: u64,
}

impl AccountHeader for HeaderWithTimestamp {
    const VERSION: u8 = 3;
}

// Set the last version of the Header struct here
// and change the `header_size` and `header_upgrade` functions
pub type Header = HeaderWithTimestamp;

pub type Storage = [[u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT];
pub type Code = [u8];

pub struct ContractAccount<'a> {
    pub account: Account<'a>, // TODO: make it private after emulator changes
}

impl<'a> ContractAccount<'a> {
    #[must_use]
    pub fn required_account_size(code: &[u8]) -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>() + size_of::<Storage>() + code.len()
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
        account.validate_tag(program_id, TAG_ACCOUNT_CONTRACT)?;

        Ok(Self { account })
    }

    pub fn initialize(
        mut account: Account<'a>,
        program_id: Pubkey,
        address: Address,
        chain_id: u64,
        code: &[u8],
    ) -> Result<Self> {
        assert_eq!(account.data_len(), Self::required_account_size(code));
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_ACCOUNT_CONTRACT, Header::VERSION)?;

        {
            let mut header: RefMut<HeaderV0> = account.header_mut();
            header.address = address;
            header.chain_id = chain_id;
            header.generation = 0;
        }
        {
            let mut header: RefMut<HeaderWithRevision> = account.header_mut();
            header.revision = 1;
        }
        {
            let mut header: RefMut<HeaderWithTimestamp> = account.header_mut();
            header.timestamp_used_at = 0;
        }

        let mut contract = Self::from_account(program_id, account)?;
        {
            let mut contract_code = contract.code_mut();
            contract_code.copy_from_slice(code);
        }

        Ok(contract)
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    fn header_size(&self) -> usize {
        match self.account.header_version() {
            0 | 1 => size_of::<HeaderV0>(),
            HeaderWithRevision::VERSION => size_of::<HeaderWithRevision>(),
            HeaderWithTimestamp::VERSION => size_of::<HeaderWithTimestamp>(),
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
            HeaderWithTimestamp::VERSION => {
                self.account
                    .expand_header::<HeaderWithTimestamp, Header>()?;
            }
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }

        Ok(())
    }

    #[inline]
    #[must_use]
    fn storage_offset(&self) -> usize {
        ACCOUNT_PREFIX_LEN + self.header_size()
    }

    #[inline]
    #[must_use]
    pub fn storage(&self) -> Ref<Storage> {
        let offset = self.storage_offset();
        self.account.section(offset)
    }

    #[inline]
    fn storage_mut(&mut self) -> RefMut<Storage> {
        let offset = self.storage_offset();
        self.account.section_mut(offset)
    }

    #[inline]
    #[must_use]
    fn code_offset(&self) -> usize {
        self.storage_offset() + size_of::<Storage>()
    }

    #[inline]
    #[must_use]
    pub fn code(&self) -> Ref<Code> {
        let offset = self.code_offset();

        let data = self.account.data();
        Ref::map(data, |d| &d[offset..])
    }

    #[inline]
    fn code_mut(&mut self) -> RefMut<Code> {
        let offset = self.code_offset();

        let data = self.account.data_mut();
        RefMut::map(data, |d| &mut d[offset..])
    }

    pub fn allocate_code(&mut self, code: &[u8]) -> Result<AllocateResult> {
        let required_size = Self::required_account_size(code);
        if self.account.data_len() >= required_size {
            return Ok(AllocateResult::Ready);
        }

        let max_size = self.account.original_data_len() + MAX_PERMITTED_DATA_INCREASE;
        let new_space = required_size.min(max_size);
        self.account.reallocate(new_space, ZeroInit::Uninit)?;

        if new_space >= required_size {
            Ok(AllocateResult::Ready)
        } else {
            Ok(AllocateResult::NeedMore)
        }
    }

    pub fn allocate_entire_code_buffer(&mut self, code: &[u8]) -> Result<()> {
        let required_size = Self::required_account_size(code);
        if self.account.data_len() >= required_size {
            return Ok(());
        }

        self.account.reallocate(required_size, ZeroInit::Uninit)
    }

    pub fn set_code(&mut self, code: &[u8]) -> Result<()> {
        {
            let mut code_region = self.code_mut();
            code_region[..code.len()].copy_from_slice(code);
            code_region[code.len()..].fill(0);
        }

        self.increment_revision()
    }

    #[must_use]
    pub fn code_len(&self) -> usize {
        let offset = self.code_offset();

        self.account.data_len().saturating_sub(offset)
    }

    #[must_use]
    pub fn address(&self) -> Address {
        let header: Ref<HeaderV0> = self.account.header();
        header.address
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        let header: Ref<HeaderV0> = self.account.header();
        header.chain_id
    }

    #[must_use]
    pub fn generation(&self) -> u32 {
        let header: Ref<HeaderV0> = self.account.header();
        header.generation
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

    #[must_use]
    pub fn timestamp_used_at(&self) -> u64 {
        if self.account.header_version() < HeaderWithTimestamp::VERSION {
            return 0;
        }

        let header: Ref<HeaderWithTimestamp> = self.account.header();
        header.timestamp_used_at
    }

    pub fn update_timestamp_used_at(&mut self, clock: &Clock) -> Result<()> {
        if self.account.header_version() < HeaderWithTimestamp::VERSION {
            self.header_upgrade()?;
        }

        let mut header: RefMut<HeaderWithTimestamp> = self.account.header_mut();
        header.timestamp_used_at = clock.slot;

        Ok(())
    }

    #[must_use]
    pub fn storage_value(&self, index: usize) -> [u8; 32] {
        assert!(index < STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT);

        let storage = self.storage();
        storage[index]
    }

    pub fn set_storage_value(&mut self, index: usize, value: &[u8; 32]) -> Result<()> {
        assert!(index < STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT);

        {
            let mut storage = self.storage_mut();
            let cell: &mut [u8; 32] = &mut storage[index];
            if cell == value {
                return Ok(());
            }

            cell.copy_from_slice(value);
        }

        self.increment_revision()
    }

    pub fn set_storage_multiple_values(&mut self, offset: usize, values: &[[u8; 32]]) {
        let max = offset.saturating_add(values.len());
        assert!(max <= STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT);

        let mut storage = self.storage_mut();
        storage[offset..][..values.len()].copy_from_slice(values);
    }
}
