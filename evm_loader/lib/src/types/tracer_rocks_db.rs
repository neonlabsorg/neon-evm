use abi_stable::traits::IntoOwned;
use async_trait::async_trait;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::Serialize;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::from_str;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

#[derive(Clone, Serialize)]
pub struct AccountParams {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::{DbResult, RocksDbConfig, TracerDb};
// use reconnecting_jsonrpsee_ws_client::{Client, CallRetryPolicy, rpc_params, ExponentialBackoff};
#[derive(Clone, Debug)]
pub struct RocksDb {
    #[allow(dead_code)]
    url: String,
    client: Arc<WsClient>,
}

impl RocksDb {
    #[must_use]
    pub async fn new(config: &RocksDbConfig) -> Self {
        let host = &config.rocksdb_host;
        let port = &config.rocksdb_port;
        let url = format!("ws://{host}:{port}");

        // match Client::builder()
        //     .retry_policy(
        //     ExponentialBackoff::from_millis(100)
        //         .max_delay(Duration::from_secs(10))
        //         .take(3),)
        match WsClientBuilder::default().build(&url).await {
            Ok(client) => {
                let arc_c = Arc::new(client);
                tracing::info!("Created rocksdb client at {url}");
                Self { url, client: arc_c }
            }
            Err(e) => panic!("Couln't start rocksDb client at {url}: {e}"),
        }
    }
}

#[async_trait]
impl TracerDb for RocksDb {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp> {
        let response: String = self
            .client
            .request("get_block_time", rpc_params![slot])
            .await?;
        info!(
            "get_block_time for slot {:?} response: {:?}",
            slot, response
        );
        Ok(i64::from_str(response.as_str())?)
    }

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64> {
        let response: String = self
            .client
            .request("get_earliest_rooted_slot", rpc_params![])
            .await?;
        info!("get_earliest_rooted_slot response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    async fn get_latest_block(&self) -> DbResult<u64> {
        let response: String = self
            .client
            .request("get_last_rooted_slot", rpc_params![])
            .await?;
        info!("get_latest_block response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> DbResult<Option<Account>> {
        info!("get_account_at {pubkey:?}, slot: {slot:?}, tx_index: {tx_index_in_block:?}");

        let response: String = self
            .client
            .request(
                "get_account",
                rpc_params![pubkey.into_owned(), slot, tx_index_in_block],
            )
            .await?;

        let account = from_str::<Option<Account>>(response.as_str())?;
        account.as_ref().map_or_else(|| {
            info!("Got None for Account by {pubkey:?}");
        }, |account| {
            info!("Got Account by {pubkey:?} owner: {:?} lamports: {:?} executable: {:?} rent_epoch: {:?}", account.owner, account.lamports, account.executable, account.rent_epoch);
        });

        Ok(account)
    }

    async fn get_transaction_index(&self, signature: Signature) -> DbResult<u64> {
        let response: String = self
            .client
            .request("get_transaction_index", rpc_params![signature])
            .await?;
        info!("get_transaction_index response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap> {
        let revision = env::var("NEON_REVISION").expect("NEON_REVISION should be set");

        info!("get_neon_revisions for {revision:?}");
        let ranges = vec![(1, 100_000, revision)];
        Ok(RevisionMap::new(ranges))
    }

    async fn get_neon_revision(&self, slot: Slot, pubkey: &Pubkey) -> DbResult<String> {
        info!("get_neon_revision for {slot:?}, pubkey: {pubkey:?}");
        Ok(env!("NEON_REVISION").to_string())
    }

    async fn get_slot_by_blockhash(&self, blockhash: String) -> DbResult<u64> {
        let response: String = self
            .client
            .request("get_slot_by_blockhash", rpc_params![blockhash])
            .await?;
        info!("response: {:?}", response);
        Ok(from_str(response.as_str())?)
    }

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus> {
        Ok(EthSyncStatus::new(None))
    }
}
