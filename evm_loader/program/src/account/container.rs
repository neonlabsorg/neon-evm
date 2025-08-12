use std::cell::{Ref, RefMut};
use std::ops::{Range, RangeFrom};

use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

use crate::account::{
    abstraction::RawAccount, AccountDispatch, AccountHeader, ACCOUNT_PREFIX_LEN, TAG_CONTAINER,
    TAG_REFERENCE,
};
use crate::account::{ZeroInit, TAG_ACCOUNT_BALANCE, TAG_ACCOUNT_CONTRACT, TAG_STORAGE_CELL};
use crate::error::{Error, Result};

const VALID_TAGS_FOR_CONTAINER: [u8; 3] =
    [TAG_ACCOUNT_BALANCE, TAG_ACCOUNT_CONTRACT, TAG_STORAGE_CELL];

#[repr(C, packed)]
struct ReferenceHeader {
    container: Pubkey,
}

impl AccountHeader for ReferenceHeader {
    const VERSION: u8 = 0;
}

pub struct ReferenceAccount<'a> {
    account: RawAccount<'a>,
}

impl<'a> ReferenceAccount<'a> {
    #[must_use]
    pub const fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<ReferenceHeader>()
    }

    pub fn from_account(program_id: Pubkey, account: RawAccount<'a>) -> Result<Self> {
        account.validate_tag(program_id, TAG_REFERENCE)?;

        Ok(Self { account })
    }

    /// # Safety
    /// It's a caller responsibility to validate the account tag
    #[must_use]
    pub unsafe fn from_account_unchecked(account: RawAccount<'a>) -> Self {
        Self { account }
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    #[must_use]
    pub fn container(&self) -> Pubkey {
        let header: Ref<ReferenceHeader> = self.account.header();
        header.container
    }
}

#[derive(Clone)]
pub struct AccountInContainer<'a> {
    pub container: ContainerAccount<'a>,
    pub index: usize,
}

// Container Account Layout
// ----------------
// Header
//   - count: u32
// ---------------
// Accounts Index - Key `count` times
// ---------------
// Accounts Data
// ---------------

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct Key {
    pub pubkey: Pubkey,
    pub offset: u32,
    pub length: u32,
}

#[repr(C, packed)]
struct ContainerHeader {
    count: u32,
}

impl AccountHeader for ContainerHeader {
    const VERSION: u8 = 0;
}

#[derive(Clone)]
pub struct ContainerAccount<'a> {
    pub account: RawAccount<'a>,
}

impl<'a> ContainerAccount<'a> {
    pub fn from_account_info(program_id: Pubkey, account_info: &AccountInfo<'a>) -> Result<Self> {
        let account = account_info.clone().into();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, account: RawAccount<'a>) -> Result<Self> {
        account.validate_tag(program_id, TAG_CONTAINER)?;

        Ok(Self { account })
    }

    /// # Safety
    /// It's a caller responsibility to validate the account tag
    #[must_use]
    pub unsafe fn from_account_unchecked(account: RawAccount<'a>) -> Self {
        Self { account }
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    #[must_use]
    fn header_size(&self) -> usize {
        match self.account.header_version() {
            ContainerHeader::VERSION => size_of::<ContainerHeader>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        let header: Ref<ContainerHeader> = self.account.header();
        header.count as usize
    }

    #[must_use]
    pub fn contains(&self, pubkey: &Pubkey) -> bool {
        let keys = self.keys();
        keys.binary_search_by_key(pubkey, |key| key.pubkey).is_ok()
    }

    #[inline]
    #[must_use]
    fn keys_section(&self) -> Range<usize> {
        let start = ACCOUNT_PREFIX_LEN + self.header_size();
        let end = start + self.count() * size_of::<Key>();
        start..end
    }

    #[inline]
    #[must_use]
    fn accounts_section(&self) -> RangeFrom<usize> {
        let keys = self.keys_section();
        keys.end..
    }

    #[must_use]
    pub fn keys(&self) -> Ref<[Key]> {
        let range = self.keys_section();
        let data: Ref<[u8]> = self.account.data_get(range);

        Ref::map(data, |bytes| {
            const { assert!(align_of::<Key>() == 1) }
            assert_eq!(bytes.len() % size_of::<Key>(), 0);

            // SAFETY: Key has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_ptr().cast::<Key>();
                let len = bytes.len() / size_of::<Key>();
                std::slice::from_raw_parts(ptr, len)
            }
        })
    }

    #[must_use]
    pub fn keys_mut(&mut self) -> RefMut<[Key]> {
        let range = self.keys_section();
        let data: RefMut<[u8]> = self.account.data_get_mut(range);

        RefMut::map(data, |bytes| {
            const { assert!(align_of::<Key>() == 1) }
            assert_eq!(bytes.len() % size_of::<Key>(), 0);

            // SAFETY: Key has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_mut_ptr().cast::<Key>();
                let len = bytes.len() / size_of::<Key>();
                std::slice::from_raw_parts_mut(ptr, len)
            }
        })
    }

    pub fn key_index(&self, pubkey: Pubkey) -> Result<usize> {
        self.keys()
            .binary_search_by_key(&pubkey, |key| key.pubkey)
            .map_err(|_| Error::AccountNotFoundInContainer(pubkey, self.pubkey()))
    }

    #[inline]
    #[must_use]
    fn key_offset(&self, index: usize) -> usize {
        if index >= self.count() {
            panic_with_error!(Error::OutOfBounds);
        }

        unsafe { self.key_offset_unchecked(index) }
    }

    #[inline]
    #[must_use]
    unsafe fn key_offset_unchecked(&self, index: usize) -> usize {
        ACCOUNT_PREFIX_LEN + self.header_size() + index * size_of::<Key>()
    }

    #[must_use]
    pub fn key_at(&self, index: usize) -> Ref<Key> {
        let offset = self.key_offset(index);
        self.account.section(offset)
    }

    #[must_use]
    pub fn key_at_mut(&mut self, index: usize) -> RefMut<Key> {
        let offset = self.key_offset(index);
        self.account.section_mut(offset)
    }

    pub fn key(&self, pubkey: Pubkey) -> Result<Ref<Key>> {
        let index = self.key_index(pubkey)?;
        let key = self.key_at(index);
        Ok(key)
    }

    #[must_use]
    pub fn accounts(&self) -> Ref<[u8]> {
        let range = self.accounts_section();
        self.account.data_get(range)
    }

    #[must_use]
    pub fn accounts_mut(&mut self) -> RefMut<[u8]> {
        let range = self.accounts_section();
        self.account.data_get_mut(range)
    }

    pub fn account(&self, pubkey: Pubkey) -> Result<AccountInContainer<'a>> {
        let index = self.key_index(pubkey)?;

        Ok(AccountInContainer {
            container: self.clone(),
            index,
        })
    }

    #[must_use]
    pub fn account_pubkey(&self, index: usize) -> Pubkey {
        let key = self.key_at(index);
        key.pubkey
    }

    #[must_use]
    pub fn account_data_len(&self, index: usize) -> usize {
        let key = self.key_at(index);
        key.length as usize
    }

    #[must_use]
    pub fn account_data(&self, index: usize) -> Ref<[u8]> {
        let key = self.key_at(index);

        let start = key.offset as usize;
        let end = start + key.length as usize;

        let accounts: Ref<[u8]> = self.accounts();
        Ref::map(accounts, |data| &data[start..end])
    }

    #[must_use]
    pub fn account_data_mut(&mut self, index: usize) -> RefMut<[u8]> {
        let key = self.key_at(index);

        let start = key.offset as usize;
        let end = start + key.length as usize;

        std::mem::drop(key);

        let accounts: RefMut<[u8]> = self.accounts_mut();
        RefMut::map(accounts, |data| &mut data[start..end])
    }

    pub fn realloc_account_data(
        &mut self,
        index: usize,
        new_size: usize,
        zero_init: ZeroInit,
    ) -> Result<()> {
        let key = *self.key_at(index);

        let current_size = key.length as usize;
        let offset = key.offset as usize;

        match new_size.cmp(&current_size) {
            std::cmp::Ordering::Equal => return Ok(()),
            std::cmp::Ordering::Greater => {
                let delta = new_size - current_size;

                // Move other accounts data to make space
                let account_end = offset + current_size;
                let account_end = self.accounts_section().start + account_end;

                self.account
                    .allocate_within(account_end, delta, zero_init)?;

                // Update keys
                let delta: u32 = delta.try_into()?;

                self.key_at_mut(index).length = new_size.try_into()?;
                for key in self.keys_mut().iter_mut() {
                    if key.offset as usize <= offset {
                        continue; // Skip keys before the current one
                    }

                    key.offset += delta;
                }
            }
            std::cmp::Ordering::Less => {
                let delta = current_size - new_size;

                // Move other accounts to make space
                let start = offset + current_size;
                let dest = start - delta;

                self.accounts_mut().copy_within(start.., dest);

                // Shrink real account
                self.account.shrink(delta)?;

                // Update keys
                let delta: u32 = delta.try_into()?;

                self.key_at_mut(index).length = new_size.try_into()?;
                for key in self.keys_mut().iter_mut() {
                    if key.offset as usize <= offset {
                        continue; // Skip keys before the current one
                    }

                    key.offset -= delta;
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn free_space_offset(&self) -> usize {
        let mut accounts_end = 0_usize;
        for key in self.keys().iter() {
            let end = (key.offset + key.length) as usize;
            if end > accounts_end {
                accounts_end = end;
            }
        }

        accounts_end
    }

    pub fn allocate_space_for_account(&mut self, space: usize) -> Result<()> {
        self.account.grow(space, ZeroInit::Uninit)
    }

    pub fn convert_from_account(program_id: Pubkey, mut account: RawAccount<'a>) -> Result<Self> {
        let tag: u8 = account.tag(program_id)?;
        if tag == TAG_CONTAINER {
            return Ok(Self { account });
        }

        let pubkey = account.pubkey();
        let account_len = account.data_len();

        if !VALID_TAGS_FOR_CONTAINER.contains(&tag) {
            return Err(Error::AccountNotSuitableForContainer(pubkey));
        }

        let key_offset = ACCOUNT_PREFIX_LEN + size_of::<ContainerHeader>();
        let account_offset = key_offset + size_of::<Key>();

        // Allocate `account_offset` bytes at the front of the account
        account.allocate_within(0, account_offset, ZeroInit::Uninit)?;

        // Set tag
        account.init_tag(TAG_CONTAINER, ContainerHeader::VERSION)?;

        // Set header
        account
            .header_mut_uninit()
            .write(ContainerHeader { count: 1 });

        // Set key
        account.section_mut_uninit(key_offset).write(Key {
            pubkey,
            offset: 0,
            length: account_len.try_into()?,
        });

        Ok(Self { account })
    }

    /// # Safety
    /// Invalidates all indexes. Should not be used in normal transaction processing.
    pub unsafe fn add_account(
        &mut self,
        program_id: Pubkey,
        mut account: RawAccount<'a>,
    ) -> Result<()> {
        let pubkey = account.pubkey();

        let tag: u8 = account.tag(program_id)?;
        if !VALID_TAGS_FOR_CONTAINER.contains(&tag) {
            return Err(Error::AccountNotSuitableForContainer(pubkey));
        }

        if self.contains(&pubkey) {
            return Err(Error::AccountAlreadyInContainer(pubkey, self.pubkey()));
        }

        let account_offset = self.free_space_offset();
        let account_len = account.data_len();

        // Allocate space for the account
        let free_space_len = self.accounts().len() - account_offset;

        if free_space_len < account_len {
            let required_space = account_len - free_space_len;
            self.allocate_space_for_account(required_space)?;
        }

        // Write account data
        self.accounts_mut()[account_offset..account_offset + account_len]
            .copy_from_slice(&account.data());

        // Allocate space for the key
        let key_index = self
            .keys()
            .binary_search_by_key(&pubkey, |key| key.pubkey)
            .unwrap_err();
        let key_offset = unsafe { self.key_offset_unchecked(key_index) };

        self.account
            .allocate_within(key_offset, size_of::<Key>(), ZeroInit::Uninit)?;

        self.account.section_mut_uninit(key_offset).write(Key {
            pubkey,
            offset: account_offset.try_into()?,
            length: account_len.try_into()?,
        });

        // Update header
        self.account.header_mut::<ContainerHeader>().count += 1;

        // Convert existing account to reference
        account.init_tag(TAG_REFERENCE, ReferenceHeader::VERSION)?;
        account.header_mut_uninit().write(ReferenceHeader {
            container: self.pubkey(),
        });
        account.reallocate(ReferenceAccount::required_account_size(), ZeroInit::Uninit)
    }
}
