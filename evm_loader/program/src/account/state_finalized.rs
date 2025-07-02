use std::cell::{Ref, RefMut};

use super::{
    AccountDispatch, AccountHeader, Operator, PlainStateHeader, StateAccount, TAG_STATE_FINALIZED,
};
use crate::{
    error::{Error, Result},
    types::{Transaction, TrxView},
};
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub transaction_hash: [u8; 32],
}

impl AccountHeader for Header {
    const VERSION: u8 = 0;
}

pub struct StateFinalizedAccount<'local, 'sol> {
    account: &'local AccountInfo<'sol>,
}

impl<'local, 'sol> StateFinalizedAccount<'local, 'sol> {
    #[must_use]
    pub fn into_account(self) -> &'local AccountInfo<'sol> {
        self.account
    }

    pub fn make(
        header: &PlainStateHeader,
        account: &'local AccountInfo<'sol>,
    ) -> Result<&'local AccountInfo<'sol>> {
        let owner = header.owner;
        let transaction_hash = header.hash();

        account.init_tag(TAG_STATE_FINALIZED, Header::VERSION)?;
        {
            let mut header: RefMut<Header> = account.header_mut();
            header.owner = owner;
            header.transaction_hash = transaction_hash;
        }

        Ok(account)
    }

    pub fn convert_from_state(
        state: StateAccount<'local, 'sol>,
    ) -> Result<&'local AccountInfo<'sol>> {
        let owner = state.owner();
        let transaction_hash = state.trx().hash();

        let account = state.into_account();

        account.init_tag(TAG_STATE_FINALIZED, Header::VERSION)?;
        {
            let mut header: RefMut<Header> = account.header_mut();
            header.owner = owner;
            header.transaction_hash = transaction_hash;
        }

        Ok(account)
    }

    pub fn from_account_info(
        program_id: Pubkey,
        account: &'local AccountInfo<'sol>,
    ) -> Result<Self> {
        account.validate_tag(program_id, TAG_STATE_FINALIZED)?;
        Ok(Self { account })
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header: Ref<Header> = self.account.header();
        header.owner
    }

    #[must_use]
    pub fn trx_hash(&self) -> [u8; 32] {
        let header: Ref<Header> = self.account.header();
        header.transaction_hash
    }

    pub fn validate_owner(&self, operator: &Operator) -> Result<()> {
        if &self.owner() != operator.key {
            return Err(Error::HolderInvalidOwner(self.owner(), *operator.key));
        }

        Ok(())
    }

    pub fn validate_trx(&self, transaction: &Transaction) -> Result<()> {
        if self.trx_hash() == transaction.hash {
            return Err(Error::StorageAccountFinalized);
        }

        Ok(())
    }
}
