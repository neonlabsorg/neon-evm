use crate::error::{Error, Result};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use std::{
    cell::{Ref, RefMut},
    collections::HashMap,
};

pub use crate::config::ACCOUNT_SEED_VERSION;

pub use ether_balance::BalanceAccount;
pub use ether_contract::{AllocateResult, ContractAccount};
pub use ether_storage::{StorageCell, StorageCellAddress};
pub use holder::Holder;
pub use incinerator::Incinerator;
pub use operator::Operator;
pub use state::StateAccount;
pub use treasury::{MainTreasury, Treasury};

use self::program::System;

mod ether_balance;
mod ether_contract;
mod ether_storage;
mod holder;
mod incinerator;
mod operator;
pub mod program;
mod state;
pub mod token;
mod treasury;

#[deprecated]
const _TAG_STATE_DEPRECATED: u8 = 22;
#[deprecated]
const _TAG_STATE_FINALIZED_DEPRECATED: u8 = 31;
#[deprecated]
const _TAG_HOLDER_DEPRECATED: u8 = 51;
#[deprecated]
const _TAG_ACCOUNT_CONTRACT_DEPRECATED: u8 = 12;
#[deprecated]
const _TAG_STORAGE_CELL_DEPRECATED: u8 = 42;

pub const TAG_EMPTY: u8 = 0;
pub const TAG_STATE: u8 = 23;
pub const TAG_STATE_FINALIZED: u8 = 32;
pub const TAG_HOLDER: u8 = 52;

const TAG_ACCOUNT_BALANCE: u8 = 60;
const TAG_ACCOUNT_CONTRACT: u8 = 70;
const TAG_STORAGE_CELL: u8 = 43;

const ACCOUNT_PREFIX_LEN: usize = 2;

#[inline]
fn section<'r, T>(account: &'r AccountInfo<'_>, offset: usize) -> Ref<'r, T> {
    let begin = offset;
    let end = begin + std::mem::size_of::<T>();

    let data = account.data.borrow();
    Ref::map(data, |d| {
        let bytes = &d[begin..end];

        assert_eq!(std::mem::align_of::<T>(), 1);
        assert_eq!(std::mem::size_of::<T>(), bytes.len());
        unsafe { &*(bytes.as_ptr() as *const T) }
    })
}

#[inline]
fn section_mut<'r, T>(account: &'r AccountInfo<'_>, offset: usize) -> RefMut<'r, T> {
    let begin = offset;
    let end = begin + std::mem::size_of::<T>();

    let data = account.data.borrow_mut();
    RefMut::map(data, |d| {
        let bytes = &mut d[begin..end];

        assert_eq!(std::mem::align_of::<T>(), 1);
        assert_eq!(std::mem::size_of::<T>(), bytes.len());
        unsafe { &mut *(bytes.as_mut_ptr() as *mut T) }
    })
}

pub fn tag(program_id: &Pubkey, info: &AccountInfo) -> Result<u8> {
    if info.owner != program_id {
        return Err(Error::AccountInvalidOwner(*info.key, *program_id));
    }

    let data = info.try_borrow_data()?;
    if data.len() < ACCOUNT_PREFIX_LEN {
        return Err(Error::AccountInvalidData(*info.key));
    }

    Ok(data[0])
}

pub fn set_tag(program_id: &Pubkey, info: &AccountInfo, tag: u8) -> Result<()> {
    assert_eq!(info.owner, program_id);

    let mut data = info.try_borrow_mut_data()?;
    assert!(data.len() >= ACCOUNT_PREFIX_LEN);

    data[0] = tag;

    Ok(())
}

pub fn validate_tag(program_id: &Pubkey, info: &AccountInfo, tag: u8) -> Result<()> {
    let account_tag = crate::account::tag(program_id, info)?;

    if account_tag == tag {
        Ok(())
    } else {
        Err(Error::AccountInvalidTag(*info.key, tag))
    }
}

pub fn is_blocked(program_id: &Pubkey, info: &AccountInfo) -> Result<bool> {
    if info.owner != program_id {
        return Err(Error::AccountInvalidOwner(*info.key, *program_id));
    }

    let data = info.try_borrow_data()?;
    if data.len() < ACCOUNT_PREFIX_LEN {
        return Err(Error::AccountInvalidData(*info.key));
    }

    Ok(data[1] == 1)
}

#[inline]
fn set_block(program_id: &Pubkey, info: &AccountInfo, block: bool) -> Result<()> {
    assert_eq!(info.owner, program_id);

    let mut data = info.try_borrow_mut_data()?;
    assert!(data.len() >= ACCOUNT_PREFIX_LEN);

    if block && (data[1] != 0) {
        return Err(Error::AccountBlocked(*info.key));
    }

    data[1] = block.into();

    Ok(())
}

pub fn block(program_id: &Pubkey, info: &AccountInfo) -> Result<()> {
    set_block(program_id, info, true)
}

pub fn unblock(program_id: &Pubkey, info: &AccountInfo) -> Result<()> {
    set_block(program_id, info, false)
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

pub struct AccountsDB<'a> {
    map: HashMap<Pubkey, AccountInfo<'a>>,
    operator: Operator<'a>,
    operator_balance: Option<BalanceAccount<'a>>,
    system: Option<System<'a>>,
    treasury: Option<Treasury<'a>>,
}

impl<'a> AccountsDB<'a> {
    pub fn new(
        accounts: &[AccountInfo<'a>],
        operator: Operator<'a>,
        operator_balance: Option<BalanceAccount<'a>>,
        system: Option<System<'a>>,
        treasury: Option<Treasury<'a>>,
    ) -> Self {
        let mut map = HashMap::with_capacity(accounts.len());

        for account in accounts {
            map.insert(*account.key, account.clone());
        }

        Self {
            map,
            operator,
            operator_balance,
            system,
            treasury,
        }
    }

    pub fn accounts_len(&self) -> usize {
        self.map.len()
    }

    pub fn system(&self) -> &System<'a> {
        if let Some(system) = &self.system {
            return system;
        }

        panic!("System Account must be present in the transaction");
    }

    pub fn treasury(&self) -> &Treasury<'a> {
        if let Some(treasury) = &self.treasury {
            return treasury;
        }

        panic!("Treasury Account must be present in the transaction");
    }

    pub fn operator(&self) -> &Operator<'a> {
        &self.operator
    }

    pub fn operator_balance(&mut self) -> &mut BalanceAccount<'a> {
        if let Some(operator_balance) = &mut self.operator_balance {
            return operator_balance;
        }

        panic!("Operator Balance Account must be present in the transaction");
    }

    pub fn operator_key(&self) -> Pubkey {
        *self.operator.key
    }

    pub fn operator_info(&self) -> &AccountInfo<'a> {
        &self.operator
    }

    pub fn get(&self, pubkey: &Pubkey) -> &AccountInfo<'a> {
        if let Some(account) = self.map.get(pubkey) {
            return account;
        }

        panic!("address {pubkey} must be present in the transaction");
    }
}

impl<'a, 'r> IntoIterator for &'r AccountsDB<'a> {
    type Item = &'r AccountInfo<'a>;
    type IntoIter = std::collections::hash_map::Values<'r, Pubkey, AccountInfo<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.values()
    }
}
