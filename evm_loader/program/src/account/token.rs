use crate::error::{Error, Result};
use solana_program::account_info::AccountInfo;
use solana_program::program_pack::{IsInitialized, Pack};
use std::ops::Deref;

pub struct Account<'a, T: Pack + IsInitialized> {
    pub info: AccountInfo<'a>,
    data: T,
}

impl<'a, T: Pack + IsInitialized> Account<'a, T> {
    pub fn from_account_info(info: &AccountInfo<'a>) -> Result<Self> {
        if !spl_token::check_id(info.owner) {
            return Err(Error::AccountInvalidOwner(*info.key, spl_token::ID));
        }

        let data = info.try_borrow_data()?;
        let data = T::unpack(&data)?;

        Ok(Self {
            info: info.clone(),
            data,
        })
    }

    pub fn into_data(self) -> T {
        self.data
    }
}

impl<T: Pack + IsInitialized> Deref for Account<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

pub type State<'a> = Account<'a, spl_token::state::Account>;
pub type Mint<'a> = Account<'a, spl_token::state::Mint>;
