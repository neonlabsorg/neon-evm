use std::cell::{Ref, RefMut};

use super::{
    AccountHeader, Operator, StateAccount, TAG_SCHEDULED_STATE_CANCELLED,
    TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE_FINALIZED,
};
use crate::{
    error::{Error, Result},
    types::Transaction,
};
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Storage data account to store finalized execution data.
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub transaction_hash: [u8; 32],
    // Not used and not written to for STATE_FINALIZED.
    // This field is private for a reason, so the logic of writing is contained by the impl.
    tree_account: Pubkey,
}

impl AccountHeader for Header {
    const VERSION: u8 = 0;
}

/// Controls the account in the `TAG_STATE_FINALIZED`, `TAG_SCHEDULED_STATE_FINALIZED`, `TAG_SCHEDULED_STATE_CANCELLED`
pub struct StateFinalizedAccount<'a> {
    account: AccountInfo<'a>,
}

impl<'a> StateFinalizedAccount<'a> {
    pub fn finalize_from_state<'s>(
        program_id: &Pubkey,
        state: StateAccount<'s>,
    ) -> Result<AccountInfo<'s>> {
        Self::finalize_from_state_impl(TAG_STATE_FINALIZED, program_id, state)
    }

    pub fn scheduled_finalize_from_state<'s>(
        program_id: &Pubkey,
        state: StateAccount<'s>,
    ) -> Result<AccountInfo<'s>> {
        Self::finalize_from_state_impl(TAG_SCHEDULED_STATE_FINALIZED, program_id, state)
    }

    pub fn scheduled_cancel_from_state<'s>(
        program_id: &Pubkey,
        state: StateAccount<'s>,
    ) -> Result<AccountInfo<'s>> {
        Self::finalize_from_state_impl(TAG_SCHEDULED_STATE_CANCELLED, program_id, state)
    }

    fn finalize_from_state_impl<'s>(
        tag: u8,
        program_id: &Pubkey,
        state: StateAccount<'s>,
    ) -> Result<AccountInfo<'s>> {
        assert!(
            tag == TAG_STATE_FINALIZED
                || tag == TAG_SCHEDULED_STATE_FINALIZED
                || tag == TAG_SCHEDULED_STATE_CANCELLED
        );
        let owner = state.owner();
        let transaction_hash = state.trx().hash();
        let tree_account = state.tree_account();

        let account = state.into_account();

        super::set_tag(program_id, &account, tag, Header::VERSION)?;
        {
            let mut header = super::header_mut::<Header>(&account);
            header.owner = owner;
            header.transaction_hash = transaction_hash;
            if tree_account.is_some() {
                assert!(
                    tag == TAG_SCHEDULED_STATE_FINALIZED || tag == TAG_SCHEDULED_STATE_CANCELLED
                );
                header.tree_account = tree_account.unwrap();
            }
            // if `tree_account` is absent, do not write anything, those bits are unused.
        }

        Ok(account)
    }

    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        Self::validate_tag(program_id, &account)?;
        Ok(Self { account })
    }

    #[inline]
    #[must_use]
    fn header(&self) -> Ref<Header> {
        super::header(&self.account)
    }

    #[inline]
    #[must_use]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::header_mut(&self.account)
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

    pub fn tree_account(&self) -> Result<Pubkey> {
        // Validation of the owner has been performed in the `from_account`.
        let tag = crate::account::tag(&crate::ID, &self.account)?;

        if tag == TAG_SCHEDULED_STATE_FINALIZED || tag == TAG_SCHEDULED_STATE_CANCELLED {
            Ok(self.header().tree_account)
        } else {
            Err(Error::FinalizedStorageAccountInvalidTag(
                *self.account.key,
                tag,
            ))
        }
    }

    fn validate_tag(program_id: &Pubkey, account: &AccountInfo<'a>) -> Result<()> {
        let tag = crate::account::tag(program_id, account)?;

        if tag == TAG_STATE_FINALIZED
            || tag == TAG_SCHEDULED_STATE_FINALIZED
            || tag == TAG_SCHEDULED_STATE_CANCELLED
        {
            Ok(())
        } else {
            Err(Error::FinalizedStorageAccountInvalidTag(*account.key, tag))
        }
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
