use crate::account_data::AccountData;
use crate::config::RocksDbConfig;
use async_trait::async_trait;
// use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::Serialize;
// use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
// use serde_json::from_str;
#[allow(dead_code)]
use crate::types::tracer_db_rpc_api::TracerDbApiClient;
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Clone, Serialize)]
pub struct AccountParams {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::{DbResult, TracerDbTrait};
// use reconnecting_jsonrpsee_ws_client::{Client, CallRetryPolicy, rpc_params, ExponentialBackoff};
#[derive(Clone, Debug)]
pub struct RocksDb {
    #[allow(dead_code)]
    url: String,
    ws_client: Arc<WsClient>,
}

impl RocksDb {
    #[must_use]
    pub async fn new(config: &RocksDbConfig) -> Self {
        let host = &config.rocksdb_host;
        let port = &config.rocksdb_port;
        let url = format!("ws://{host}:{port}");
        match WsClientBuilder::default().build(&url).await {
            Ok(client) => {
                let arc_c = Arc::new(client);
                tracing::info!("Created rocksdb client at {url}");
                Self {
                    url,
                    ws_client: arc_c,
                }
            }
            Err(e) => panic!("Couldn't start rocksDb client at {url}: {e}"),
        }
    }
}

#[async_trait]
impl TracerDbTrait for RocksDb {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp> {
        let block_time = self.ws_client.get_block_time(slot).await?;
        if let Some(block_time) = block_time {
            return Ok(block_time);
        }
        anyhow::bail!("Block time value None")
    }

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64> {
        Ok(self.ws_client.get_earliest_rooted_slot().await?)
    }

    async fn get_latest_block(&self) -> DbResult<u64> {
        Ok(self.ws_client.get_earliest_rooted_slot().await?)
    }

    async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
        maybe_bin_slice: Option<UiDataSliceConfig>,
    ) -> DbResult<Option<Account>> {
        info!("get_account_at {pubkey:?}, slot: {slot:?}, tx_index: {tx_index_in_block:?}, bin_slice: {maybe_bin_slice:?}");
        Ok(self
            .ws_client
            .get_account_at(
                &pubkey.to_string(),
                slot,
                tx_index_in_block,
                maybe_bin_slice,
            )
            .await?)
    }

    async fn get_transaction_index(&self, signature: Signature) -> DbResult<u64> {
        let tx_index = self
            .ws_client
            .get_transaction_index(&signature.to_string())
            .await?;
        if let Some(tx_index) = tx_index {
            return Ok(tx_index);
        }
        anyhow::bail!("get_transaction_index value is None")
    }

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap> {
        let revision = env::var("NEON_REVISION").expect("NEON_REVISION should be set");

        info!("get_neon_revisions for {revision:?}");
        let ranges = vec![(1, 100_000, revision)];
        Ok(RevisionMap::new(ranges))
    }

    async fn get_neon_revision(&self, slot: Slot, pubkey: &Pubkey) -> DbResult<String> {
        info!("get_neon_revision for {slot:?}, pubkey: {pubkey:?}");
        let neon_revision = env!("NEON_REVISION");
        Ok(neon_revision.to_string())
    }

    async fn get_slot_by_blockhash(&self, blockhash: String) -> DbResult<u64> {
        let slot = self
            .ws_client
            .get_slot_by_blockhash(blockhash.as_str())
            .await?;
        if let Some(slot) = slot {
            return Ok(slot);
        }
        anyhow::bail!("get_slot_by_blockhash value is None")
    }

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus> {
        Ok(EthSyncStatus::new(None))
    }

    async fn get_accounts_in_transaction(
        &self,
        sol_sig: &[u8],
        slot: u64,
    ) -> DbResult<Vec<AccountData>> {
        let signature = Signature::try_from(sol_sig)?;

        let response: Vec<(String, Account)> = self
            .ws_client
            .get_accounts_in_transaction(&signature.to_string(), Some(slot))
            .await?;
        debug!("Accounts in response: {:?}", response);
        let account_data_vec = response
            .iter()
            .map(|(pubkey, acc)| {
                AccountData::new_from_account(Pubkey::from_str(pubkey).unwrap(), acc)
            })
            .collect();
        Ok(account_data_vec)
    }
}
