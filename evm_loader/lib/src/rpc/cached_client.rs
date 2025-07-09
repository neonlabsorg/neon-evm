use std::{cell::RefCell, collections::HashMap};

use async_trait::async_trait;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::{account::Account, pubkey::Pubkey};

use super::{Rpc, SliceConfig};

pub struct CachedRpc<R: Rpc> {
    rpc: R,
    accounts_cache: RefCell<HashMap<Pubkey, Option<Account>>>,
    deativated_features: RefCell<Option<Vec<Pubkey>>>,
}

impl<R: Rpc> CachedRpc<R> {
    pub fn new(rpc: R) -> Self {
        Self {
            rpc,
            accounts_cache: RefCell::new(HashMap::new()),
            deativated_features: RefCell::new(None),
        }
    }

    pub fn get_from_cache(&self, key: &Pubkey) -> Option<Option<Account>> {
        self.accounts_cache.borrow().get(key).cloned()
    }

    pub fn insert_to_cache(&self, key: Pubkey, account: Option<Account>) {
        self.accounts_cache.borrow_mut().insert(key, account);
    }

    pub async fn cache_accounts(&self, pubkeys: &[Pubkey]) {
        let _ = self.get_multiple_accounts(pubkeys).await;
    }

    fn get_cached_features(&self) -> Option<Vec<Pubkey>> {
        self.deativated_features.borrow().clone()
    }

    fn cache_features(&self, features: Vec<Pubkey>) {
        *self.deativated_features.borrow_mut() = Some(features);
    }
}

#[async_trait(?Send)]
impl<R: Rpc> Rpc for CachedRpc<R> {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        let Some(mut account) = self.get_account(key).await? else {
            return Ok(None);
        };

        if let Some(slice) = slice {
            let range = slice.offset..(slice.offset + slice.length);
            account.data.copy_within(range, 0);
            account.data.truncate(slice.length);
        }

        Ok(Some(account))
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
        if let Some(account) = self.get_from_cache(key) {
            Ok(account)
        } else {
            let account = self.rpc.get_account(key).await?;
            self.insert_to_cache(*key, account.clone());

            Ok(account)
        }
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        let mut accounts: Vec<Option<Account>> = vec![None; pubkeys.len()];

        let mut exists = vec![true; pubkeys.len()];
        let mut missing_keys = Vec::with_capacity(pubkeys.len());

        for (i, pubkey) in pubkeys.iter().enumerate() {
            if let Some(account_data) = self.get_from_cache(pubkey) {
                accounts[i] = account_data;
                continue;
            }

            exists[i] = false;
            missing_keys.push(*pubkey);
        }

        let mut response = self.rpc.get_multiple_accounts(&missing_keys).await?;

        let mut j = 0_usize;
        for i in 0..pubkeys.len() {
            if exists[i] {
                continue;
            }

            assert_eq!(pubkeys[i], missing_keys[j]);
            accounts[i] = response[j].take();

            self.insert_to_cache(pubkeys[i], accounts[i].clone());

            j += 1;
        }

        Ok(accounts)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        if let Some(features) = self.get_cached_features() {
            return Ok(features);
        }

        let features = self.rpc.get_deactivated_solana_features().await?;
        self.cache_features(features.clone());

        Ok(features)
    }
}
