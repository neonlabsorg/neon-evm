use std::cell::{Ref, RefMut};

use super::{ACCOUNT_PREFIX_LEN, TAG_STATE_FINALIZED};
use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub transaction_hash: [u8; 32],
}

pub struct StateFinalizedAccount<'a> {
    account: AccountInfo<'a>,
}

const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;

impl<'a> StateFinalizedAccount<'a> {
    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, &account, TAG_STATE_FINALIZED)?;
        Ok(Self { account })
    }

    #[inline]
    #[must_use]
    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    #[inline]
    #[must_use]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Header),
    {
        let mut header = self.header_mut();
        f(&mut header);
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        self.header().owner
    }

    #[must_use]
    pub fn trx_hash(&self) -> [u8; 32] {
        self.header().transaction_hash
    }
}
