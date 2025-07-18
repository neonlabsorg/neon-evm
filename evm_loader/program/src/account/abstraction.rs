use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    mem::MaybeUninit,
    rc::Rc,
    slice::SliceIndex,
};

use enum_dispatch::enum_dispatch;
use solana_account::ReadableAccount;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, system_program};

use crate::{
    account::AccountInContainer,
    error::{Error, Result},
};

const TAG_OFFSET: usize = 0;
const HEADER_VERSION_OFFSET: usize = 1;
pub const ACCOUNT_PREFIX_LEN: usize = 1/*tag*/ + 1/*header version*/;

#[derive(Clone)]
pub struct SharedAccount {
    key: Pubkey,
    original_data_len: usize,
    backup: Rc<solana_account::Account>,
    account: Rc<RefCell<solana_account::Account>>,
    modified: Rc<Cell<bool>>,
}

impl SharedAccount {
    #[must_use]
    pub fn new(pubkey: Pubkey, account: &impl ReadableAccount) -> Self {
        let account: solana_account::Account = account.to_account_shared_data().into();

        Self {
            key: pubkey,
            original_data_len: account.data.len(),
            backup: Rc::new(account.clone()),
            account: Rc::new(RefCell::new(account)),
            modified: Rc::new(Cell::new(false)),
        }
    }

    #[must_use]
    pub fn new_empty(pubkey: Pubkey) -> Self {
        let empty_account = solana_account::Account::default();
        Self::new(pubkey, &empty_account)
    }

    #[must_use]
    pub fn deep_clone(&self) -> Self {
        let account = self.account.borrow().clone();

        Self {
            key: self.key,
            original_data_len: self.original_data_len,
            backup: Rc::new(account.clone()),
            account: Rc::new(RefCell::new(account)),
            modified: Rc::clone(&self.modified), // modified once, modified everywhere
        }
    }

    pub fn mark_modified(&self) {
        self.modified.set(true);
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified.get()
    }

    pub fn assign(&self, owner: Pubkey) {
        let mut account = self.account.borrow_mut();
        account.owner = owner;
    }

    pub fn update(&self, other: &impl ReadableAccount) {
        let mut account = self.account.borrow_mut();
        account.owner = *other.owner();
        account.data = other.data().to_vec();
        account.lamports = other.lamports();
        account.executable = other.executable();
    }

    pub fn revert(&self) {
        let backup = solana_account::Account::clone(&self.backup);
        self.account.replace(backup);
    }
}

impl From<&SharedAccount> for solana_account::Account {
    fn from(account: &SharedAccount) -> Self {
        Self {
            lamports: account.lamports(),
            data: account.data().to_vec(),
            owner: account.owner(),
            executable: account.is_executable(),
            rent_epoch: account.rent_epoch(),
        }
    }
}

#[enum_dispatch]
#[derive(Clone)]
pub enum RawAccount<'a> {
    AccountInfo(AccountInfo<'a>),
    #[cfg(not(target_os = "solana"))]
    SharedAccount(SharedAccount),
}

#[enum_dispatch]
#[derive(Clone)]
pub enum Account<'a> {
    RawAccount(RawAccount<'a>),
    AccountInContainer(AccountInContainer<'a>),
}

#[cfg(not(target_os = "solana"))]
impl From<SharedAccount> for Account<'_> {
    fn from(account: SharedAccount) -> Self {
        let raw_account = RawAccount::SharedAccount(account);
        Self::RawAccount(raw_account)
    }
}

impl<'a> From<AccountInfo<'a>> for Account<'a> {
    fn from(account: AccountInfo<'a>) -> Self {
        let raw_account = RawAccount::AccountInfo(account);
        Self::RawAccount(raw_account)
    }
}

impl<'a> TryFrom<Account<'a>> for AccountInfo<'a> {
    type Error = &'static str;

    fn try_from(account: Account<'a>) -> std::result::Result<Self, Self::Error> {
        let raw_account: RawAccount<'a> = account.try_into()?;
        raw_account.try_into()
    }
}

impl<'a> Account<'a> {
    #[must_use]
    #[allow(irrefutable_let_patterns)]
    pub const fn as_account_info(&self) -> &AccountInfo<'a> {
        let Account::RawAccount(solana_account) = self else {
            panic!("Account expected to be AccountInfo")
        };

        let RawAccount::AccountInfo(account_info) = solana_account else {
            panic!("Account expected to be AccountInfo")
        };

        account_info
    }
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
#[enum_dispatch(RawAccount)]
pub trait AccountDispatch<'a> {
    fn data(&self) -> Ref<[u8]>;
    fn data_mut(&mut self) -> RefMut<[u8]>;

    fn data_len(&self) -> usize;
    fn original_data_len(&self) -> usize;

    fn pubkey(&self) -> Pubkey;
    fn owner(&self) -> Pubkey;
    fn is_system_owned(&self) -> bool;

    fn lamports(&self) -> u64;
    fn rent_epoch(&self) -> u64;
    fn is_executable(&self) -> bool;

    fn reallocate(&mut self, new_size: usize, zero_init: ZeroInit) -> Result<()>;

    fn grow(&mut self, grow_by: usize, zero_init: ZeroInit) -> Result<()> {
        let new_size = self.data_len().saturating_add(grow_by);
        self.reallocate(new_size, zero_init)
    }

    fn shrink(&mut self, shrink_by: usize) -> Result<()> {
        let new_size = self.data_len().saturating_sub(shrink_by);
        self.reallocate(new_size, ZeroInit::Uninit)
    }

    fn allocate_within(&mut self, offset: usize, len: usize, zero_init: ZeroInit) -> Result<()> {
        self.grow(len, ZeroInit::Uninit)?;

        // Move data to the right
        let end = self.data_len() - len;
        let dest = offset + len;

        let mut data = self.data_mut();
        data.copy_within(offset..end, dest);

        // Fill the new space with zeros if requested
        if zero_init == ZeroInit::Zero {
            data[offset..offset + len].fill(0);
        }

        Ok(())
    }

    #[inline]
    fn data_get<I: SliceIndex<[u8]>>(&self, index: I) -> Ref<I::Output> {
        Ref::map(self.data(), |data| &data[index])
    }

    #[inline]
    fn data_get_mut<I: SliceIndex<[u8]>>(&mut self, index: I) -> RefMut<I::Output> {
        RefMut::map(self.data_mut(), |data| &mut data[index])
    }

    fn tag(&self, program_id: Pubkey) -> Result<u8> {
        if self.owner() != program_id {
            return Err(Error::AccountInvalidOwner(self.pubkey(), program_id));
        }

        let data = self.data();
        if data.len() < ACCOUNT_PREFIX_LEN {
            return Err(Error::AccountInvalidData(self.pubkey()));
        }

        let account_tag = unsafe { data.get_unchecked(TAG_OFFSET) };
        Ok(*account_tag)
    }

    fn tag_is(&self, program_id: Pubkey, tag: u8) -> bool {
        if self.owner() != program_id {
            return false;
        }

        let data = self.data();
        if data.len() < ACCOUNT_PREFIX_LEN {
            return false;
        }

        let account_tag = unsafe { data.get_unchecked(TAG_OFFSET) };
        account_tag == &tag
    }

    fn validate_tag(&self, program_id: Pubkey, tag: u8) -> Result<()> {
        if self.tag_is(program_id, tag) {
            Ok(())
        } else {
            Err(Error::AccountInvalidTag(self.pubkey(), tag))
        }
    }

    fn init_tag(&mut self, tag: u8, header_version: u8) -> Result<()> {
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
    fn header_mut<T: AccountHeader>(&mut self) -> RefMut<T> {
        self.section_mut(ACCOUNT_PREFIX_LEN)
    }

    #[inline]
    fn header_mut_uninit<T: AccountHeader>(&mut self) -> RefMut<MaybeUninit<T>> {
        self.section_mut_uninit(ACCOUNT_PREFIX_LEN)
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

        self.reallocate(required_len, ZeroInit::Uninit)?;

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

    fn data_mut(&mut self) -> RefMut<[u8]> {
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
}

impl<'a> AccountDispatch<'a> for AccountInContainer<'a> {
    fn data(&self) -> Ref<[u8]> {
        self.container.account_data(self.index)
    }

    fn data_mut(&mut self) -> RefMut<[u8]> {
        self.container.account_data_mut(self.index)
    }

    fn data_len(&self) -> usize {
        self.container.account_data_len(self.index)
    }

    fn original_data_len(&self) -> usize {
        unreachable!() // Should not be used
    }

    fn pubkey(&self) -> Pubkey {
        self.container.account_pubkey(self.index)
    }

    fn owner(&self) -> Pubkey {
        self.container.account.owner()
    }

    fn is_system_owned(&self) -> bool {
        false
    }

    fn lamports(&self) -> u64 {
        self.container.account.lamports()
    }

    fn rent_epoch(&self) -> u64 {
        self.container.account.rent_epoch()
    }

    fn is_executable(&self) -> bool {
        false
    }

    fn reallocate(&mut self, new_size: usize, zero_init: ZeroInit) -> Result<()> {
        self.container
            .realloc_account_data(self.index, new_size, zero_init)
    }
}

impl AccountDispatch<'_> for SharedAccount {
    fn data(&self) -> Ref<[u8]> {
        let account = self.account.borrow();
        Ref::map(account, |a| a.data.as_slice())
    }

    fn data_mut(&mut self) -> RefMut<[u8]> {
        self.modified.set(true);

        let account = self.account.borrow_mut();
        RefMut::map(account, |a| a.data.as_mut_slice())
    }

    fn data_len(&self) -> usize {
        let account = self.account.borrow();
        account.data.len()
    }

    fn original_data_len(&self) -> usize {
        self.original_data_len
    }

    fn pubkey(&self) -> Pubkey {
        self.key
    }

    fn owner(&self) -> Pubkey {
        let account = self.account.borrow();
        account.owner
    }

    fn is_system_owned(&self) -> bool {
        let account = self.account.borrow();
        solana_program::system_program::check_id(&account.owner)
    }

    fn lamports(&self) -> u64 {
        let account = self.account.borrow();
        account.lamports
    }

    fn rent_epoch(&self) -> u64 {
        let account = self.account.borrow();
        account.rent_epoch
    }

    fn is_executable(&self) -> bool {
        let account = self.account.borrow();
        account.executable
    }

    fn reallocate(&mut self, new_size: usize, _: ZeroInit) -> Result<()> {
        self.modified.set(true);

        let mut account = self.account.borrow_mut();
        account.data.resize(new_size, 0); // No limits on SharedAccount reallocation

        Ok(())
    }
}
