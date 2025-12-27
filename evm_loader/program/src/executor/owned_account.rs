use solana_program::pubkey::Pubkey;

use crate::account::AccountRead;

#[derive(Clone, Default)]
#[repr(C)]
pub struct OwnedAccountInfo {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
}

impl OwnedAccountInfo {
    #[must_use]
    pub fn from_account(info: &impl AccountRead) -> Self {
        Self {
            lamports: info.lamports(),
            data: info.data().to_vec(),
            owner: info.owner(),
        }
    }
}
