use std::{cell::RefCell, rc::Rc};

use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::{account_storage::AccountStorage, types::{tree_map::BTreeMap, vector::{into_vector, Vector}}, vector};

#[derive(Clone)]
pub struct OwnedAccountInfo {
    pub key: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
    pub lamports: u64,
    pub data: Vector<u8>,
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: solana_program::clock::Epoch,
}

impl OwnedAccountInfo {
    #[must_use]
    pub fn from_account_info(program_id: &Pubkey, info: &AccountInfo) -> Self {
        Self {
            key: *info.key,
            is_signer: info.is_signer,
            is_writable: info.is_writable,
            lamports: info.lamports(),
            data: if info.executable || (info.owner == program_id) {
                // This is only used to emulate external programs
                // They don't use data in our accounts
                vector![]
            } else {
                into_vector(info.data.borrow().to_vec())
            },
            owner: *info.owner,
            executable: info.executable,
            rent_epoch: info.rent_epoch,
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

pub struct Cache {
    pub solana_accounts: BTreeMap<Pubkey, OwnedAccountInfo>,
    pub block_number: U256,
    pub block_timestamp: U256,
}

#[maybe_async]
#[allow(clippy::await_holding_refcell_ref)] // We don't use this RefCell<Cache> in other execution context
pub async fn cache_get_or_insert_account<B: AccountStorage>(
    cache: &RefCell<Cache>,
    key: Pubkey,
    backend: &B,
) -> OwnedAccountInfo {
    let mut cache = cache.borrow_mut();
    match cache.solana_accounts.get(&key) {
        None => {
            let owned_account_info = backend.clone_solana_account(&key).await;
            cache.solana_accounts.insert(key, &owned_account_info);
            owned_account_info.clone()
        }
        Some(value) => value.clone(),
    }
}
