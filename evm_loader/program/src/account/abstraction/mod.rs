use std::{
    cell::{Ref, RefMut},
    mem::MaybeUninit,
    slice::SliceIndex,
};

use solana_program::pubkey::Pubkey;

use crate::error::{Error, Result};

mod abstract_account;
mod account_in_container;
mod account_info;

pub use abstract_account::AbstractAccount;

const TAG_OFFSET: usize = 0;
const HEADER_VERSION_OFFSET: usize = 1;
pub const ACCOUNT_PREFIX_LEN: usize = 1/*tag*/ + 1/*header version*/;

pub trait AccountHeader {
    const VERSION: u8;
}
pub struct NoHeader {}
impl AccountHeader for NoHeader {
    const VERSION: u8 = 0;
}

pub trait AccountRead {
    fn data(&self) -> Ref<[u8]>;

    fn data_len(&self) -> usize;
    fn original_data_len(&self) -> usize;

    fn pubkey(&self) -> Pubkey;
    fn container(&self) -> Option<Pubkey>;

    fn owner(&self) -> Pubkey;
    fn is_system_owned(&self) -> bool;

    fn lamports(&self) -> u64;
    fn rent_epoch(&self) -> u64;
    fn is_executable(&self) -> bool;

    fn memory_address(&self) -> u64 {
        self.data().as_ptr().addr() as u64
    }

    #[inline]
    fn data_get<I: SliceIndex<[u8]>>(&self, index: I) -> Ref<I::Output> {
        Ref::map(self.data(), |data| &data[index])
    }

    fn tag(&self, program_id: &Pubkey) -> Result<u8> {
        if &self.owner() != program_id {
            return Err(Error::AccountInvalidOwner(self.pubkey(), *program_id));
        }

        let data = self.data();
        if data.len() < ACCOUNT_PREFIX_LEN {
            return Err(Error::AccountInvalidData(self.pubkey()));
        }

        let account_tag = unsafe { data.get_unchecked(TAG_OFFSET) };
        Ok(*account_tag)
    }

    fn tag_is(&self, program_id: &Pubkey, tag: u8) -> bool {
        if &self.owner() != program_id {
            return false;
        }

        let data = self.data();
        if data.len() < ACCOUNT_PREFIX_LEN {
            return false;
        }

        let account_tag = unsafe { data.get_unchecked(TAG_OFFSET) };
        account_tag == &tag
    }

    fn validate_tag(&self, program_id: &Pubkey, tag: u8) -> Result<()> {
        if self.tag_is(program_id, tag) {
            Ok(())
        } else {
            Err(Error::AccountInvalidTag(self.pubkey(), tag))
        }
    }

    #[inline]
    fn section<T>(&self, offset: usize) -> Ref<T> {
        let begin = offset;
        let end = begin + std::mem::size_of::<T>();

        let data = self.data();
        Ref::map(data, |d| {
            // Ensure that the entire range is within account data
            let bytes = &d[begin..end];
            assert_eq!(std::mem::size_of::<T>(), bytes.len());

            let ptr = bytes.as_ptr().cast::<T>();
            assert!(ptr.is_aligned());

            // SAFETY: The pointer is not null, aligned and convertible to reference
            unsafe { &*ptr }
        })
    }

    #[inline]
    fn header<T: AccountHeader>(&self) -> Ref<T> {
        self.section(ACCOUNT_PREFIX_LEN)
    }

    fn header_version(&self) -> u8 {
        // This is used only inside the module and account validation should be already done
        let data = self.data();
        data[HEADER_VERSION_OFFSET]
    }
}
pub trait AccountWrite {
    fn data_mut(&mut self) -> RefMut<[u8]>;
    fn reallocate(&mut self, new_size: usize) -> Result<()>;

    #[inline]
    fn data_get_mut<I: SliceIndex<[u8]>>(&mut self, index: I) -> RefMut<I::Output> {
        RefMut::map(self.data_mut(), |data| &mut data[index])
    }

    fn write_tag(&mut self, tag: u8, header_version: u8) -> Result<()> {
        let mut data = self.data_mut();
        assert!(data.len() >= ACCOUNT_PREFIX_LEN);

        data[TAG_OFFSET] = tag;
        data[HEADER_VERSION_OFFSET] = header_version;

        Ok(())
    }

    fn replace_tag(&mut self, tag: u8) -> Result<()> {
        let mut data = self.data_mut();
        assert!(data.len() >= ACCOUNT_PREFIX_LEN);

        data[TAG_OFFSET] = tag;

        Ok(())
    }

    #[inline]
    fn section_mut<T>(&mut self, offset: usize) -> RefMut<T> {
        let begin = offset;
        let end = begin + std::mem::size_of::<T>();

        let data = self.data_mut();
        RefMut::map(data, |d| {
            let bytes = &mut d[begin..end];
            assert_eq!(std::mem::size_of::<T>(), bytes.len());

            let ptr = bytes.as_mut_ptr().cast::<T>();
            assert!(ptr.is_aligned());

            unsafe { &mut *ptr }
        })
    }

    #[inline]
    fn section_mut_uninit<T>(&mut self, offset: usize) -> RefMut<MaybeUninit<T>> {
        let begin = offset;
        let end = begin + std::mem::size_of::<T>();

        let data = self.data_mut();
        RefMut::map(data, |d| {
            let bytes = &mut d[begin..end];
            assert_eq!(std::mem::size_of::<MaybeUninit<T>>(), bytes.len());

            let ptr = bytes.as_mut_ptr().cast::<MaybeUninit<T>>();
            assert!(ptr.is_aligned());

            unsafe { &mut *ptr }
        })
    }

    #[inline]
    fn header_mut<T: AccountHeader>(&mut self) -> RefMut<T> {
        self.section_mut(ACCOUNT_PREFIX_LEN)
    }

    #[inline]
    fn write_header<T: AccountHeader>(&mut self, header: T) {
        self.section_mut_uninit(ACCOUNT_PREFIX_LEN).write(header);
    }
}

pub trait Account: AccountRead + AccountWrite {
    #[inline]
    /// # Safety
    /// It is the caller's responsibility to not dereference the pointer outside of the account data bounds.
    unsafe fn data_mut_ptr(&mut self, offset: usize) -> *mut u8 {
        assert!(offset < self.data_len());

        let mut data = self.data_mut();
        data.as_mut_ptr().add(offset)
    }

    fn grow(&mut self, grow_by: usize) -> Result<()> {
        let new_size = self.data_len().saturating_add(grow_by);
        self.reallocate(new_size)
    }

    fn shrink(&mut self, shrink_by: usize) -> Result<()> {
        let new_size = self.data_len().saturating_sub(shrink_by);
        self.reallocate(new_size)
    }

    fn allocate_within(&mut self, offset: usize, len: usize) -> Result<()> {
        self.grow(len)?;

        // Move data to the right
        let end = self.data_len() - len;
        let dest = offset + len;

        let mut data = self.data_mut();
        data.copy_within(offset..end, dest);

        // Fill the new space with zeros
        data[offset..offset + len].fill(0);

        Ok(())
    }

    fn expand_header<From: AccountHeader, To: AccountHeader>(&mut self) -> Result<()> {
        let from_len = std::mem::size_of::<From>();
        let to_len = std::mem::size_of::<To>();

        let data_len = self.data_len();

        assert!(to_len >= from_len);
        assert!(data_len >= ACCOUNT_PREFIX_LEN + from_len);

        let data_len = data_len - ACCOUNT_PREFIX_LEN - from_len;
        let required_len = ACCOUNT_PREFIX_LEN + to_len + data_len;
        assert!(required_len >= data_len);

        self.reallocate(required_len)?;

        {
            let mut account_data = self.data_mut();

            let begin = ACCOUNT_PREFIX_LEN + from_len;
            let end = begin + data_len;
            let target = ACCOUNT_PREFIX_LEN + to_len;
            account_data.copy_within(begin..end, target);
            account_data[begin..target].fill(0);
            account_data[HEADER_VERSION_OFFSET] = To::VERSION;
        }

        Ok(())
    }
}
