use std::{
    cell::{Ref, RefMut},
    mem::MaybeUninit,
};

use enum_dispatch::enum_dispatch;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, system_program};

use crate::error::{Error, Result};

const TAG_OFFSET: usize = 0;
const HEADER_VERSION_OFFSET: usize = 1;
pub const ACCOUNT_PREFIX_LEN: usize = 1/*tag*/ + 1/*header version*/;

#[enum_dispatch]
#[derive(Clone)]
pub enum Account<'a> {
    AccountInfo(AccountInfo<'a>),
}

#[derive(PartialEq, Eq)]
pub enum ZeroInit {
    Zero,
    Uninit,
}

pub trait AccountHeader {
    const VERSION: u8;
}
pub struct NoHeader {}
impl AccountHeader for NoHeader {
    const VERSION: u8 = 0;
}

#[enum_dispatch(Account)]
pub trait AccountDispatch<'a> {
    fn data(&self) -> Ref<[u8]>;
    fn data_mut(&self) -> RefMut<[u8]>; // TODO Make self mutable after state.rs refactor

    fn data_len(&self) -> usize;
    fn original_data_len(&self) -> usize;

    fn pubkey(&self) -> Pubkey;
    fn owner(&self) -> Pubkey;
    fn is_system_owned(&self) -> bool;

    fn lamports(&self) -> u64;
    fn rent_epoch(&self) -> u64;
    fn is_executable(&self) -> bool;

    fn reallocate(&mut self, new_size: usize, zero_init: ZeroInit) -> Result<()>;
    fn sync_lamports(&mut self, rent: &Rent, db: &super::AccountsDB<'a>) -> Result<()>;

    fn tag(&self, program_id: Pubkey) -> Result<u8> {
        if self.owner() != program_id {
            return Err(Error::AccountInvalidOwner(self.pubkey(), program_id));
        }

        let data = self.data();
        if data.len() < ACCOUNT_PREFIX_LEN {
            return Err(Error::AccountInvalidData(self.pubkey()));
        }

        Ok(data[TAG_OFFSET])
    }

    fn validate_tag(&self, program_id: Pubkey, tag: u8) -> Result<()> {
        let account_tag = self.tag(program_id)?;

        if account_tag == tag {
            Ok(())
        } else {
            Err(Error::AccountInvalidTag(self.pubkey(), tag))
        }
    }

    // TODO Make self mutable after state.rs refactor
    fn init_tag(&self, tag: u8, header_version: u8) -> Result<()> {
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
    // TODO Make self mutable after state.rs refactor
    fn section_mut<T>(&self, offset: usize) -> RefMut<T> {
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
    /// # Safety
    /// It is the caller's responsibility to not dereference the pointer outside of the account data bounds.
    unsafe fn data_mut_ptr(&mut self, offset: usize) -> *mut u8 {
        assert!(offset < self.data_len());

        let mut data = self.data_mut();
        data.as_mut_ptr().add(offset)
    }

    fn memory_address(&self) -> u64 {
        self.data().as_ptr().addr() as u64
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

    #[inline]
    fn header_mut<T: AccountHeader>(&self) -> RefMut<T> {
        // TODO Make self mutable after state.rs refactor
        self.section_mut(ACCOUNT_PREFIX_LEN)
    }

    #[inline]
    fn header_mut_uninit<T: AccountHeader>(&mut self) -> RefMut<MaybeUninit<T>> {
        self.section_mut_uninit(ACCOUNT_PREFIX_LEN)
    }

    fn expand_header<From: AccountHeader, To: AccountHeader>(
        &mut self,
        rent: &Rent,
        db: &super::AccountsDB<'a>,
    ) -> Result<()> {
        let from_len = std::mem::size_of::<From>();
        let to_len = std::mem::size_of::<To>();

        let data_len = self.data_len();

        assert!(to_len >= from_len);
        assert!(data_len >= ACCOUNT_PREFIX_LEN + from_len);

        let data_len = data_len - ACCOUNT_PREFIX_LEN - from_len;
        let required_len = ACCOUNT_PREFIX_LEN + to_len + data_len;
        assert!(required_len >= data_len);

        self.reallocate(required_len, ZeroInit::Uninit)?;
        self.sync_lamports(rent, db)?;

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

impl<'a> AccountDispatch<'a> for AccountInfo<'a> {
    fn data(&self) -> Ref<[u8]> {
        let data = self.data.borrow();
        Ref::map(data, |data| &data[..])
    }

    fn data_mut(&self) -> RefMut<[u8]> {
        let data = self.data.borrow_mut();
        RefMut::map(data, |data| &mut data[..])
    }

    fn data_len(&self) -> usize {
        self.data.borrow().len()
    }

    fn original_data_len(&self) -> usize {
        unsafe { self.original_data_len() }
    }

    fn pubkey(&self) -> Pubkey {
        *self.key
    }

    fn owner(&self) -> Pubkey {
        *self.owner
    }

    fn is_system_owned(&self) -> bool {
        system_program::check_id(self.owner)
    }

    fn lamports(&self) -> u64 {
        **self.lamports.borrow()
    }

    fn rent_epoch(&self) -> u64 {
        self.rent_epoch
    }

    fn is_executable(&self) -> bool {
        self.executable
    }

    fn reallocate(&mut self, new_size: usize, zero_init: ZeroInit) -> Result<()> {
        self.realloc(new_size, zero_init == ZeroInit::Zero)?;

        Ok(())
    }

    fn sync_lamports(&mut self, rent: &Rent, accounts: &super::AccountsDB<'a>) -> Result<()> {
        let original_data_len = unsafe { self.original_data_len() };
        if original_data_len == self.data_len() {
            return Ok(());
        }

        let minimum_balance = rent.minimum_balance(self.data_len());
        if self.lamports() >= minimum_balance {
            return Ok(());
        }

        let system = accounts.system();
        let operator = accounts.operator();

        let lamports = minimum_balance - self.lamports();
        system.transfer(operator, self, lamports)?;

        Ok(())
    }
}
