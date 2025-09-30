use std::cell::{Ref, RefMut};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use super::{Account, AccountRead, AccountWrite};
use crate::error::Result;

impl AccountRead for AccountInfo<'_> {
    fn data(&self) -> Ref<[u8]> {
        let data = self.data.borrow();
        Ref::map(data, |data| &data[..])
    }

    fn data_len(&self) -> usize {
        self.data.borrow().len()
    }

    fn original_data_len(&self) -> usize {
        unsafe { AccountInfo::original_data_len(self) }
    }

    fn pubkey(&self) -> Pubkey {
        *self.key
    }

    fn container(&self) -> Option<Pubkey> {
        None
    }

    fn owner(&self) -> Pubkey {
        *self.owner
    }

    fn is_system_owned(&self) -> bool {
        solana_sdk_ids::system_program::check_id(self.owner)
    }

    fn lamports(&self) -> u64 {
        **self.lamports.borrow()
    }

    fn rent_epoch(&self) -> u64 {
        self.rent_epoch
    }

    fn is_executable(&self) -> bool {
        self.executable
    }
}

impl AccountWrite for AccountInfo<'_> {
    fn data_mut(&mut self) -> RefMut<[u8]> {
        let data = self.data.borrow_mut();
        RefMut::map(data, |data| &mut data[..])
    }

    fn reallocate(&mut self, new_size: usize) -> Result<()> {
        self.resize(new_size)?;

        Ok(())
    }
}

impl Account for AccountInfo<'_> {}
