use abi_stable::traits::IntoOwned;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::Serialize;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::{from_slice, from_str};
use std::str::FromStr;
use std::sync::Arc;

pub type RocksDbResult<T> = std::result::Result<T, anyhow::Error>;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use tracing::info;

#[derive(Clone, Serialize)]
pub struct AccountParams {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::RocksDbConfig;

// use reconnecting_jsonrpsee_ws_client::{Client, CallRetryPolicy, rpc_params, ExponentialBackoff};

#[derive(Clone)]
pub struct RocksDb {
    pub url: String,
    pub client: Arc<WsClient>,
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

    pub async fn get_block_time(&self, slot: Slot) -> RocksDbResult<UnixTimestamp> {
        let response: String = self
            .client
            .request("get_block_time", rpc_params![slot])
            .await?;
        tracing::info!(
            "get_block_time for slot {:?} response: {:?}",
            slot,
            response
        );
        Ok(i64::from_str(response.as_str())?)
    }

    pub async fn get_earliest_rooted_slot(&self) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_earliest_rooted_slot", rpc_params![])
            .await?;
        tracing::info!("get_earliest_rooted_slot response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_latest_block(&self) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_last_rooted_slot", rpc_params![])
            .await?;
        tracing::info!("get_latest_block response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> RocksDbResult<Option<Account>> {
        info!("get_account_at {pubkey:?}, slot: {slot:?}, tx_index: {tx_index_in_block:?}");

        let response: String = self
            .client
            .request(
                "get_account",
                rpc_params![pubkey.into_owned(), slot, tx_index_in_block],
            )
            .await?;

        let account = from_str::<Option<Account>>(response.as_str())?;
        if let Some(account) = &account {
            info!("Got Account by {pubkey:?} owner: {:?} lamports: {:?} executable: {:?} rent_epoch: {:?}", account.owner, account.lamports, account.executable, account.rent_epoch);
        } else {
            info!("Got None for Account by {pubkey:?}");
        }
        Ok(account)
    }

    pub async fn get_transaction_index(&self, signature: Signature) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_transaction_index", rpc_params![signature])
            .await?;
        println!("get_transaction_index response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_accounts(&self, start: u64, end: u64) -> RocksDbResult<Vec<Vec<u8>>> {
        let response: String = self
            .client
            .request("get_accounts", rpc_params![start, end])
            .await?;
        tracing::info!("get_accounts response: {:?}", response);
        let accounts: Vec<Vec<u8>> = from_slice((response).as_ref()).unwrap();
        Ok(accounts)
    }

    // TODO: Implement
    // These are used by Tracer directly and eventually need to be implemented

    pub async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> RocksDbResult<RevisionMap> {
        let revision = env!("NEON_REVISION").to_string();
        let ranges = vec![(1, 100_000, revision)];
        Ok(RevisionMap::new(ranges))
    }

    pub async fn get_neon_revision(&self, _slot: Slot, _pubkey: &Pubkey) -> RocksDbResult<String> {
        Ok(env!("NEON_REVISION").to_string())
    }

    pub async fn get_slot_by_blockhash(&self, blockhash: String) -> RocksDbResult<Option<u64>> {
        let response: String = self
            .client
            .request("get_slot_by_blockhash", rpc_params![blockhash])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(from_str(response.as_str())?)
    }

    pub async fn get_sync_status(&self) -> RocksDbResult<EthSyncStatus> {
        Ok(EthSyncStatus::new(None))
    }
}
