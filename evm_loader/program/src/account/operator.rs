// use crate::error::Error;
use solana_program::account_info::AccountInfo;
use solana_program::program_error::ProgramError;
use std::ops::Deref;

#[derive(Clone)]
pub struct Operator<'a> {
    pub info: &'a AccountInfo<'a>,
}

impl<'a> Operator<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self, ProgramError> {
        if !solana_program::system_program::check_id(info.owner) {
            return Err!(ProgramError::InvalidArgument; "Account {} - expected system owned", info.key);
        }

        if !info.is_signer {
            return Err!(ProgramError::InvalidArgument; "Account {} - expected signer", info.key);
        }

        if info.data_len() > 0 {
            return Err!(ProgramError::InvalidArgument; "Account {} - expected empty", info.key);
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
