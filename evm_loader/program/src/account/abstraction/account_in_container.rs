use std::cell::{Ref, RefMut};

use solana_program::pubkey::Pubkey;

use super::{Account, AccountRead, AccountWrite};
use crate::{account::AccountInContainer, error::Result};

impl<T: AccountRead> AccountRead for AccountInContainer<T> {
    fn data(&self) -> Ref<[u8]> {
        self.container.account_data(self.index)
    }

    fn data_len(&self) -> usize {
        self.container.account_data_len(self.index)
    }

    fn original_data_len(&self) -> usize {
        unreachable!() // Should not be used
    }

    fn pubkey(&self) -> Pubkey {
        self.container.account_pubkey(self.index)
    }

    fn container(&self) -> Option<Pubkey> {
        Some(self.container.pubkey())
    }

    fn owner(&self) -> Pubkey {
        self.container.account.owner()
    }

    fn is_system_owned(&self) -> bool {
        false
    }

    fn lamports(&self) -> u64 {
        self.container.account.lamports()
    }

    fn rent_epoch(&self) -> u64 {
        self.container.account.rent_epoch()
    }

    fn is_executable(&self) -> bool {
        false
    }
}

impl<T: Account> AccountWrite for AccountInContainer<T> {
    fn data_mut(&mut self) -> RefMut<[u8]> {
        self.container.account_data_mut(self.index)
    }

    fn reallocate(&mut self, new_size: usize) -> Result<()> {
        self.container.realloc_account_data(self.index, new_size)
    }
}

impl<T: Account> Account for AccountInContainer<T> {}
