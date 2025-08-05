use solana_program::{account_info::AccountInfo, pubkey::Pubkey};
use std::{cell::RefCell, rc::Rc};

use crate::{
    account::{Account, AccountDispatch},
    platform::FAKE_OPERATOR,
};

#[derive(Clone, Default)]
#[repr(C)]
pub struct OwnedAccountInfo {
    pub key: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: solana_program::clock::Epoch,
}

impl OwnedAccountInfo {
    #[must_use]
    pub fn from_account(program_id: Pubkey, info: &Account) -> Self {
        Self {
            key: info.pubkey(),
            is_signer: false,
            is_writable: false,
            lamports: info.lamports(),
            data: if info.is_executable() || (info.owner() == program_id) {
                // This is only used to emulate external programs
                // They don't use data in our accounts
                vec![]
            } else {
                info.data().to_vec()
            },
            owner: info.owner(),
            executable: info.is_executable(),
            rent_epoch: info.rent_epoch(),
        }
    }

    #[must_use]
    pub fn fake_operator() -> Self {
        Self {
            key: FAKE_OPERATOR,
            is_signer: true,
            is_writable: true,
            lamports: 100 * 1_000_000_000,
            data: vec![],
            owner: solana_sdk_ids::system_program::ID,
            executable: false,
            rent_epoch: u64::MAX,
        }
    }
}

impl<'a> solana_program::account_info::IntoAccountInfo<'a> for &'a mut OwnedAccountInfo {
    fn into_account_info(self) -> AccountInfo<'a> {
        AccountInfo {
            key: &self.key,
            is_signer: self.is_signer,
            is_writable: self.is_writable,
            lamports: Rc::new(RefCell::new(&mut self.lamports)),
            data: Rc::new(RefCell::new(&mut self.data)),
            owner: &self.owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
        }
    }
}
