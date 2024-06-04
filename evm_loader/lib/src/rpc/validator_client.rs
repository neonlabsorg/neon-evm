use crate::{config::APIOptions, Config};

use super::Rpc;
use async_trait::async_trait;
use solana_client::{
    client_error::Result as ClientResult, nonblocking::rpc_client::RpcClient,
    rpc_response::RpcResult,
};
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone)]
pub struct CloneRpcClient {
    pub rpc: Arc<RpcClient>,
    pub key_for_config: Pubkey,
}

impl CloneRpcClient {
    #[must_use]
    pub fn new_from_config(config: &Config) -> Self {
        let url = config.json_rpc_url.clone();
        let commitment = config.commitment;

        let rpc_client = RpcClient::new_with_commitment(url, commitment);
        Self {
            rpc: Arc::new(rpc_client),
            key_for_config: config.key_for_config,
        }
    }

    #[must_use]
    pub fn new_from_api_config(config: &APIOptions) -> Self {
        let url = config.json_rpc_url.clone();
        let commitment = config.commitment;

        let rpc_client = RpcClient::new_with_commitment(url, commitment);
        Self {
            rpc: Arc::new(rpc_client),
            key_for_config: config.key_for_config,
        }
    }
}

impl Deref for CloneRpcClient {
    type Target = RpcClient;

    fn deref(&self) -> &Self::Target {
        &self.rpc
    }
}

#[async_trait(?Send)]
impl Rpc for CloneRpcClient {
    async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>> {
        self.rpc
            .get_account_with_commitment(key, self.commitment())
            .await
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        let mut result: Vec<Option<Account>> = Vec::new();
        for chunk in pubkeys.chunks(100) {
            let mut accounts = self.rpc.get_multiple_accounts(chunk).await?;
            result.append(&mut accounts);
        }

        Ok(result)
    }

    async fn get_block_time(&self, slot: Slot) -> ClientResult<UnixTimestamp> {
        self.rpc.get_block_time(slot).await
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        self.rpc.get_slot().await
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        use std::time::{Duration, Instant};
        use tokio::sync::Mutex;

        struct Cache {
            data: Vec<Pubkey>,
            timestamp: Instant,
        }

        static CACHE: Mutex<Option<Cache>> = Mutex::const_new(None);
        let mut cache = CACHE.lock().await;

        if let Some(cache) = cache.as_ref() {
            if cache.timestamp.elapsed() < Duration::from_secs(24 * 60 * 60) {
                return Ok(cache.data.clone());
            }
        }

        let feature_keys: Vec<Pubkey> = solana_sdk::feature_set::FEATURE_NAMES
            .keys()
            .copied()
            .collect();

        let features = Rpc::get_multiple_accounts(self, &feature_keys).await?;

        let mut result = Vec::with_capacity(feature_keys.len());
        for (pubkey, feature) in feature_keys.iter().zip(features) {
            let is_activated = feature
                .and_then(|a| solana_sdk::feature::from_account(&a))
                .and_then(|f| f.activated_at)
                .is_some();

            if !is_activated {
                result.push(*pubkey);
            }
        }

        cache.replace(Cache {
            data: result.clone(),
            timestamp: Instant::now(),
        });
        drop(cache);

        Ok(result)
    }
}
