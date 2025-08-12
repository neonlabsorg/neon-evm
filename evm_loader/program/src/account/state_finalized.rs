use std::cell::{Ref, RefMut};

use super::{AccountDispatch, AccountHeader, Operator, StateAccount, TAG_STATE_FINALIZED};
use crate::{
    error::{Error, Result},
    types::EncodedTransaction,
};
use ethnum::U256;
use solana_program::{account_info::AccountInfo, keccak, pubkey::Pubkey};

/// Storage data account to store execution metainfo between steps for iterative execution
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub hash: [u8; 32],
}

impl AccountHeader for Header {
    const VERSION: u8 = 0;
}

pub struct StateFinalizedAccount<'sol> {
    account: AccountInfo<'sol>,
}

impl<'sol> StateFinalizedAccount<'sol> {
    #[must_use]
    pub fn into_account(self) -> AccountInfo<'sol> {
        self.account
    }

    pub fn convert_from_state(state: StateAccount<'sol>) -> Result<Self> {
        // Ensure that all gas is used or returned
        // Gas will disappear if this is false
        assert_eq!(state.root().gas_available(), U256::ZERO);

        let owner = state.owner();
        let hash = state.transaction_hash().to_bytes();

        let mut account = state.into_account();

        account.init_tag(TAG_STATE_FINALIZED, Header::VERSION)?;
        {
            let mut header = account.header_mut_uninit();
            header.write(Header { owner, hash });
        }

        Ok(Self { account })
    }

    pub fn from_account_info(program_id: Pubkey, account_info: &AccountInfo<'sol>) -> Result<Self> {
        let account = account_info.clone();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, account: AccountInfo<'sol>) -> Result<Self> {
        account.validate_tag(program_id, TAG_STATE_FINALIZED)?;
        Ok(Self { account })
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(RefMut<Header>),
    {
        let header: RefMut<Header> = self.account.header_mut();
        f(header);
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header: Ref<Header> = self.account.header();
        header.owner
    }

    #[must_use]
    pub fn trx_hash(&self) -> keccak::Hash {
        let header: Ref<Header> = self.account.header();
        keccak::Hash(header.hash)
    }

    pub fn validate_owner(&self, operator: &Operator) -> Result<()> {
        if &self.owner() != operator.key {
            return Err(Error::HolderInvalidOwner(self.owner(), *operator.key));
        }

        Ok(())
    }

    pub fn validate_trx(&self, transaction: &EncodedTransaction) -> Result<()> {
        if &self.trx_hash() == transaction.hash() {
            return Err(Error::StorageAccountFinalized);
        }

        Ok(())
    }
}
