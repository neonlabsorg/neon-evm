use evm_loader::account::ether_contract::INTERNAL_STORAGE_SIZE;
use evm_loader::account::{ether_account, Packable};
use evm_loader::error::{Error, Result};
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

// TODO: Make immutable borrow
pub type EthereumAccountOwned = AccountDataOwned<ether_account::Data>;

#[derive(Debug)]
pub struct AccountDataOwned<T>
where
    T: Packable + Debug,
{
    dirty: bool,
    data: T,
    _key: Pubkey,
    pub info: Account,
}

impl<T> AccountDataOwned<T>
where
    T: Packable + Debug,
{
    pub const SIZE: usize = 1 + T::SIZE;
    pub const TAG: u8 = T::TAG;

    pub fn from_account(program_id: Pubkey, key: Pubkey, info: Account) -> Result<Self> {
        if info.owner != program_id {
            return Err(Error::AccountInvalidOwner(key, program_id));
        }

        let parts = split_account_data(key, &info.data[..], T::SIZE)?;
        if *parts.tag != T::TAG {
            return Err(Error::AccountInvalidTag(key, T::TAG));
        }

        let data = T::unpack(parts.data);

        Ok(Self {
            dirty: false,
            data,
            _key: key,
            info,
        })
    }
}

fn split_account_data(key: Pubkey, account_data: &[u8], data_len: usize) -> Result<AccountParts> {
    if account_data.len() < 1 + data_len {
        return Err(Error::AccountInvalidData(key));
    }

    let (tag, bytes) = account_data.split_first().expect("data is not empty");
    let (data, remaining) = bytes.split_at(data_len);

    Ok(AccountParts {
        tag,
        data,
        _remaining: remaining,
    })
}

struct AccountParts<'a> {
    tag: &'a u8,
    data: &'a [u8],
    _remaining: &'a [u8],
}

impl<T> Deref for AccountDataOwned<T>
where
    T: Packable + Debug,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for AccountDataOwned<T>
where
    T: Packable + Debug,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.data
    }
}

pub struct ContractData<'a> {
    account: &'a EthereumAccountOwned,
}

impl EthereumAccountOwned {
    #[must_use]
    pub fn is_contract(&self) -> bool {
        self.code_size() != 0
    }

    #[must_use]
    pub fn code_size(&self) -> usize {
        self.code_size as usize
    }

    #[must_use]
    pub fn contract_data(&self) -> Option<ContractData> {
        if !self.is_contract() {
            return None;
        }
        Some(ContractData { account: self })
    }
}

impl ContractData<'_> {
    #[must_use]
    pub fn code(&self) -> &[u8] {
        let offset = INTERNAL_STORAGE_SIZE;
        let len = self.account.data.code_size as usize;

        &self.account.info.data[EthereumAccountOwned::SIZE..][offset..][..len]
    }

    #[must_use]
    pub fn storage(&self) -> &[u8] {
        let offset = 0;
        let len = INTERNAL_STORAGE_SIZE;

        &self.account.info.data[EthereumAccountOwned::SIZE..][offset..][..len]
    }
}
