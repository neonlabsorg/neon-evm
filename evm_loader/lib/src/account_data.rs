pub use evm_loader::account_storage::{AccountStorage, SyncedAccountStorage};
use solana_sdk::system_program;
use solana_sdk::{account::Account, account_info::AccountInfo, pubkey::Pubkey};

#[derive(Clone, Debug)]
#[repr(C)]
pub struct AccountData {
    original_length: u32,
    pub pubkey: Pubkey,
    pub lamports: u64,
    data: Vec<u8>,
    pub owner: Pubkey,
}

use solana_sdk::account_info::IntoAccountInfo;
use solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE;

impl AccountData {
    pub fn new(pubkey: Pubkey) -> Self {
        Self {
            original_length: 0,
            pubkey,
            lamports: 0,
            data: vec![0u8; 8 + MAX_PERMITTED_DATA_INCREASE],
            owner: system_program::ID,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.get_length() == 0 && self.owner == system_program::ID
    }

    pub fn is_busy(&self) -> bool {
        self.get_length() != 0 || self.owner != system_program::ID
    }

    pub fn new_from_account(pubkey: Pubkey, account: &Account) -> Self {
        let mut data = vec![0u8; account.data.len() + 8 + MAX_PERMITTED_DATA_INCREASE];
        let ptr_length = data.as_mut_ptr() as *mut _ as *mut u64;
        unsafe { *ptr_length = account.data.len() as u64 };
        data[8..8 + account.data.len()].copy_from_slice(&account.data);

        Self {
            original_length: account.data.len() as u32,
            pubkey,
            lamports: account.lamports,
            data,
            owner: account.owner,
        }
    }

    pub fn expand(&mut self, length: usize) {
        if self.original_length < length as u32 {
            self.data
                .resize(length + 8 + MAX_PERMITTED_DATA_INCREASE, 0);
            self.original_length = length as u32;
        }
        let ptr_length = self.data.as_mut_ptr() as *mut _ as *mut u64;
        unsafe {
            if *ptr_length < length as u64 {
                *ptr_length = length as u64;
            }
        }
    }

    pub fn reserve(&mut self) {
        self.expand(self.get_length())
    }

    pub fn assign(&mut self, owner: Pubkey) -> evm_loader::error::Result<()> {
        if self.owner != system_program::ID {
            return Err(evm_loader::error::Error::AccountAlreadyInitialized(
                self.pubkey,
            ));
        }
        self.owner = owner;
        Ok(())
    }

    pub fn data(&self) -> &[u8] {
        let length = self.get_length();
        &self.data[8..8 + length]
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        let length = self.get_length();
        &mut self.data[8..8 + length]
    }

    pub fn get_length(&self) -> usize {
        let ptr_length = self.data.as_ptr() as *const _ as *const u64;
        (unsafe { *ptr_length }) as usize
    }

    fn get(&mut self) -> (&Pubkey, &mut u64, &mut [u8], &Pubkey, bool, u64) {
        let length = self.get_length();
        (
            &self.pubkey,
            &mut self.lamports,
            &mut self.data[8..8 + length],
            &self.owner,
            false,
            0,
        )
    }
}

impl<'a> IntoAccountInfo<'a> for &'a mut AccountData {
    fn into_account_info(self) -> AccountInfo<'a> {
        let (pubkey, lamports, data, owner, executable, rent_epoch) = self.get();

        AccountInfo::new(
            pubkey, false, false, lamports, data, owner, executable, rent_epoch,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_account_data() {
        let mut account_data = AccountData::new(Pubkey::default());
        let new_owner = Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
        let new_size: usize = 10 * 1024;

        {
            let account_info = (&mut account_data).into_account_info();
            assert_eq!(account_info.try_data_len().unwrap(), 0);
            account_info.realloc(new_size - 1, false).unwrap();
            account_info.assign(&new_owner);
        }

        assert_eq!(account_data.get_length(), new_size - 1);

        {
            let account_info = (&mut account_data).into_account_info();
            assert_eq!(account_info.try_data_len().unwrap(), new_size - 1);
            assert_eq!(account_info.realloc(new_size, false), Ok(()));
            assert_eq!(
                account_info.realloc(new_size + 1, false),
                Err(solana_sdk::program_error::ProgramError::InvalidRealloc)
            );
            let mut lamports = account_info.try_borrow_mut_lamports().unwrap();
            **lamports = 10000;
        }

        assert_eq!(account_data.get_length(), new_size);
        assert_eq!(account_data.owner, new_owner);
        assert_eq!(account_data.lamports, 10000);

        {
            let account_info = (&mut account_data).into_account_info();
            account_info.realloc(0, false).unwrap();
            account_info.assign(&Pubkey::default());
        }
        assert_eq!(account_data.get_length(), 0);
        assert_eq!(account_data.owner, Pubkey::default());
    }
}
