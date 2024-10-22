use solana_program::account_info::AccountInfo;
use solana_program::system_program;

use crate::error::Error;
use crate::error::Result;
use std::ops::Deref;

#[derive(Clone)]
pub struct Signer<'a> {
    pub info: &'a AccountInfo<'a>,
}

impl<'a> Signer<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self> {
        if !system_program::check_id(info.owner) {
            return Err(Error::AccountInvalidOwner(*info.key, system_program::ID));
        }

        if !info.is_signer {
            return Err(Error::AccountNotSigner(*info.key));
        }

        if info.data_len() > 0 {
            return Err(Error::AccountInvalidData(*info.key));
        }

        Ok(Self { info })
    }
}

impl<'a> Deref for Signer<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}
