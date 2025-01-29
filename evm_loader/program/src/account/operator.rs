use crate::error::{Error, Result};
use solana_program::account_info::AccountInfo;
use solana_program::system_program;
use std::ops::Deref;

#[derive(Clone)]
pub struct Operator<'a> {
    pub info: &'a AccountInfo<'a>,
}

impl<'a> Operator<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self> {
        let is_authorized = crate::config::AUTHORIZED_OPERATOR_LIST
            .binary_search(info.key)
            .is_ok();

        if !is_authorized {
            return Err(Error::UnauthorizedOperator);
        }

        unsafe { Self::from_account_not_whitelisted(info) }
    }

    /// # Safety
    /// Due to critical vulnerability, operator can destroy the world
    /// We trust whitelisted operators to not do this
    pub unsafe fn from_account_not_whitelisted(info: &'a AccountInfo<'a>) -> Result<Self> {
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

impl<'a> Deref for Operator<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}
