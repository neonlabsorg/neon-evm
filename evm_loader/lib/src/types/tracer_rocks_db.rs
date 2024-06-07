use jsonrpsee::core::client::ClientT;
use std::str::FromStr;
use jsonrpsee::core::Serialize;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::from_str;

#[derive(Error, Debug)]
pub enum RocksDbError {
    #[error("clickhouse: {}", .0)]
    Db(#[from] rocksdb::Error),
}

// TODO: uncomment whith RPC Client
// pub type RocksDbResult<T> = std::result::Result<T, RocksDbError>;
pub type RocksDbResult<T> = std::result::Result<T, anyhow::Error>;

use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};

#[derive(Clone, Serialize)]
pub struct AccountParams{
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use thiserror::Error;
use crate::types::RocksDbConfig;

#[derive(Clone)]
pub struct RocksDb {
    // pub storage : RocksDBStorage,
    // TODO: store client here, not url
    // pub client: &WsClient,
    pub url: String,
}

impl RocksDb {
    #[must_use]
    pub fn new(config: &RocksDbConfig) -> Self {
        let addr = &config.rocksdb_url;
        let url = format!("ws://{}", addr);

        Self { url }
    }

    pub async fn get_block_time(&self, slot: Slot) -> RocksDbResult<UnixTimestamp> {
        // self.storage.get_block(slot).unwrap()?.block_time.unwrap()?
        let client: WsClient = WsClientBuilder::default().build(&self.url).await?;
        let response : String = client.request("get_block_time", rpc_params![slot]).await?;
        tracing::info!("response: {:?}", response);
        Ok(i64::from_str(response.as_str())?)
    }
    pub async fn get_earliest_rooted_slot(&self) -> RocksDbResult<u64> {
        // self.storage.get_earliest_rooted_slot()
        let client: WsClient = WsClientBuilder::default().build(&self.url).await?;
        let response : String = client.request("get_earliest_rooted_slot", rpc_params![]).await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_latest_block(&self) -> RocksDbResult<u64> {
        // self.storage.get_latest_slot()
        let client: WsClient = WsClientBuilder::default().build(&self.url).await?;
        let response : String = client.request("get_last_rooted_slot", rpc_params![]).await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> RocksDbResult<Option<Account>> {
        let ap: AccountParams = AccountParams { pubkey: *pubkey, slot, tx_index_in_block};

        let client: WsClient = WsClientBuilder::default().build(&self.url).await?;
        let response : String = client.request("get_account", rpc_params![ap]).await?;
        tracing::info!("response: {:?}", response);

        if let Some(account) = from_str(response.as_str())? {
            Ok(Some(account))
        }
        else {
            Ok(None)
        }
    }
}

