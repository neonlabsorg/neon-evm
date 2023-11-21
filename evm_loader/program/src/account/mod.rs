use std::cell::RefMut;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

use crate::error::{Error, Result};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;

pub use crate::config::ACCOUNT_SEED_VERSION;

pub use incinerator::Incinerator;
pub use operator::Operator;
pub use treasury::{MainTreasury, Treasury};

pub mod ether_account;
pub mod ether_contract;
pub mod ether_storage;
pub mod holder;
mod incinerator;
mod operator;
pub mod program;
pub mod state;
pub mod sysvar;
pub mod token;
mod treasury;

/*
Deprecated tags:

const TAG_ACCOUNT_V1: u8 = 1;
const TAG_ACCOUNT_V2: u8 = 10;
const TAG_CONTRACT_V1: u8 = 2;
const TAG_CONTRACT_V2: u8 = 20;
const TAG_CONTRACT_STORAGE: u8 = 6;
const TAG_STATE_V1: u8 = 3;
const TAG_STATE_V2: u8 = 30;
const TAG_STATE_V3: u8 = 21;
const TAG_ERC20_ALLOWANCE: u8 = 4;
const TAG_FINALIZED_STATE: u8 = 5;
const TAG_HOLDER: u8 = 6;
*/

pub const TAG_EMPTY: u8 = 0;
const TAG_ACCOUNT_V3: u8 = 12;
const TAG_STATE: u8 = 22;
const TAG_FINALIZED_STATE: u8 = 31;
const TAG_CONTRACT_STORAGE: u8 = 42;
const TAG_HOLDER: u8 = 51;

pub type EthereumAccount<'a> = AccountData<'a, ether_account::Data>;
pub type EthereumStorage<'a> = AccountData<'a, ether_storage::Data>;
pub type State<'a> = AccountData<'a, state::Data>;
pub type FinalizedState<'a> = AccountData<'a, state::FinalizedData>;
pub type Holder<'a> = AccountData<'a, holder::Data>;

pub trait Packable {
    const TAG: u8;
    const SIZE: usize;

    fn unpack(data: &[u8]) -> Self;
    fn pack(&self, data: &mut [u8]);
}

struct AccountParts<'a> {
    tag: RefMut<'a, u8>,
    data: RefMut<'a, [u8]>,
    remaining: RefMut<'a, [u8]>,
}

#[derive(Debug)]
pub struct AccountData<'a, T>
where
    T: Packable + Debug,
{
    dirty: bool,
    data: T,
    pub info: &'a AccountInfo<'a>,
}

fn split_account_data<'a>(info: &'a AccountInfo<'a>, data_len: usize) -> Result<AccountParts> {
    if info.data_len() < 1 + data_len {
        return Err(Error::AccountInvalidData(*info.key));
    }

    let account_data = info.try_borrow_mut_data()?;
    let (tag, bytes) = RefMut::map_split(account_data, |d| {
        d.split_first_mut().expect("data is not empty")
    });
    let (data, remaining) = RefMut::map_split(bytes, |d| d.split_at_mut(data_len));

    Ok(AccountParts {
        tag,
        data,
        remaining,
    })
}

impl<'a, T> AccountData<'a, T>
where
    T: Packable + Debug,
{
    pub const SIZE: usize = 1 + T::SIZE;
    pub const TAG: u8 = T::TAG;

    pub fn from_account(program_id: &Pubkey, info: &'a AccountInfo<'a>) -> Result<Self> {
        Ok(Self {
            dirty: false,
            data: Self::from_account_info(program_id, info)?,
            info,
        })
    }

    pub fn from_account_info(program_id: &Pubkey, info: &'a AccountInfo<'a>) -> Result<T> {
        if info.owner != program_id {
            return Err(Error::AccountInvalidOwner(*info.key, *program_id));
        }

        let parts = split_account_data(info, T::SIZE)?;
        if *parts.tag != T::TAG {
            return Err(Error::AccountInvalidTag(*info.key, T::TAG));
        }

        Ok(T::unpack(&parts.data))
    }

    pub fn init(program_id: &Pubkey, info: &'a AccountInfo<'a>, data: T) -> Result<Self> {
        if info.owner != program_id {
            return Err(Error::AccountInvalidOwner(*info.key, *program_id));
        }

        if !info.is_writable {
            return Err(Error::AccountNotWritable(*info.key));
        }

        let rent = Rent::get()?;
        if !rent.is_exempt(info.lamports(), info.data_len()) {
            return Err(Error::AccountNotRentExempt(*info.key));
        }

        let mut parts = split_account_data(info, T::SIZE)?;
        if *parts.tag != TAG_EMPTY {
            return Err(Error::AccountAlreadyInitialized(*info.key));
        }

        *parts.tag = T::TAG;
        data.pack(&mut parts.data);

        parts.remaining.fill(0);

        Ok(Self {
            dirty: false,
            data,
            info,
        })
    }

    /// # Safety
    /// *Delete account*. Transfer lamports to the operator.
    /// All data stored in the account will be lost
    pub unsafe fn suicide(mut self, operator: &Operator<'a>) {
        let info = self.info;

        self.dirty = false; // Do not save data into solana account
        core::mem::drop(self); // Release borrowed account data

        crate::account::delete(info, operator);
    }

    /// # Safety
    /// Should be used with care. Can corrupt account data
    pub unsafe fn replace<U>(mut self, data: U) -> Result<AccountData<'a, U>>
    where
        U: Packable + Debug,
    {
        debug_print!("replace account data from {:?} to {:?}", &self.data, &data);
        let info = self.info;

        if !info.is_writable {
            return Err(Error::AccountNotWritable(*info.key));
        }

        self.dirty = false; // Do not save data into solana account
        core::mem::drop(self); // Release borrowed account data

        let mut parts = split_account_data(info, U::SIZE)?;

        *parts.tag = U::TAG;
        data.pack(&mut parts.data);

        parts.remaining.fill(0);

        Ok(AccountData {
            dirty: false,
            data,
            info,
        })
    }
}

impl<'a, T> Deref for AccountData<'a, T>
where
    T: Packable + Debug,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<'a, T> DerefMut for AccountData<'a, T>
where
    T: Packable + Debug,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.data
    }
}

impl<'a, T> Drop for AccountData<'a, T>
where
    T: Packable + Debug,
{
    fn drop(&mut self) {
        if !self.dirty {
            return;
        }

        debug_print!("Save into solana account {:?}", self.data);
        assert!(self.info.is_writable);

        let mut parts =
            split_account_data(self.info, T::SIZE).expect("Account have incorrect size");

        self.data.pack(&mut parts.data);
    }
}

pub fn tag(program_id: &Pubkey, info: &AccountInfo) -> Result<u8> {
    if info.owner != program_id {
        return Err(Error::AccountInvalidOwner(*info.key, *program_id));
    }

    let data = info.try_borrow_data()?;
    if data.is_empty() {
        return Err(Error::AccountInvalidData(*info.key));
    }

    Ok(data[0])
}

/// # Safety
/// *Permanently delete all data* in the account. Transfer lamports to the operator.
pub unsafe fn delete(account: &AccountInfo, operator: &Operator) {
    debug_print!("DELETE ACCOUNT {}", account.key);

    **operator.lamports.borrow_mut() += account.lamports();
    **account.lamports.borrow_mut() = 0;

    let mut data = account.data.borrow_mut();
    data.fill(0);
}
