use jsonrpsee::core::client::ClientT;
use std::str::FromStr;
use std::sync::Arc;
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
// use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};

#[derive(Clone)]
pub struct RocksDb {
    // pub storage : RocksDBStorage,
    pub url: String,
    pub client: Arc<WsClient>,
}

impl RocksDb {
    #[must_use]
    pub async fn new(config: &RocksDbConfig) -> Self {
        let addr = &config.rocksdb_url;
        let url = format!("ws://{}", addr);

        match WsClientBuilder::default().build(&url).await {
            Ok(client) => {
                let arc_c = Arc::new(client);
                Self { url, client: arc_c }
            },
            Err(e) => panic!("Couln't start rocksDb client: {}", e)
        }
    }

    pub async fn get_block_time(&self, slot: Slot) -> RocksDbResult<UnixTimestamp> {
        // self.storage.get_block(slot).unwrap()?.block_time.unwrap()?
        let response: String = self.client.request("get_block_time", rpc_params![slot]).await?;
        tracing::info!("response: {:?}", response);
        Ok(i64::from_str(response.as_str())?)
    }
    pub async fn get_earliest_rooted_slot(&self) -> RocksDbResult<u64> {
        // self.storage.get_earliest_rooted_slot()
        let response: String = self.client.request("get_earliest_rooted_slot", rpc_params![]).await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_latest_block(&self) -> RocksDbResult<u64> {
        // self.storage.get_latest_slot()
        let response: String = self.client.request("get_last_rooted_slot", rpc_params![]).await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> RocksDbResult<Option<Account>> {
        let ap: AccountParams = AccountParams { pubkey: *pubkey, slot, tx_index_in_block };

        let response: String = self.client.request("get_account", rpc_params![ap]).await?;
        tracing::info!("response: {:?}", response);

        if let Some(account) = from_str(response.as_str())? {
            Ok(Some(account))
        } else {
            Ok(None)
        }
    }


    // TODO: These are used by Tracer directly and either need to be implemented or dependency on them redesigned
    // for Tracer to work against RocksDb instead of Clickhouse

    // pub async fn get_neon_revisions(&self, pubkey: &Pubkey) -> RocksDbResult<RevisionMap> {
    //  TODO implement
    // }


    // pub async fn get_neon_revision(&self, slot: Slot, pubkey: &Pubkey) -> RocksDbResult<String> {
    //     // TODO implement
    // }

    pub async fn get_slot_by_blockhash(&self, blockhash: &str) -> RocksDbResult<u64> {
        let response: String = self.client.request("get_slot_by_blockhash", rpc_params![blockhash]).await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    // pub async fn get_sync_status(&self) -> RocksDbResult<EthSyncStatus> {
    // TODO implement ?
    // }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use jsonrpsee::RpcModule;
    use jsonrpsee::server::ServerBuilder;
    use rocksdb_storage::rocksdb_structs::RootedSlot;
    use rocksdb_storage::rpc::rocksdb_service;
    use rocksdb_storage::rpc::rocksdb_service::Storage;
    use rocksdb_storage::storage::{DBStorage, RocksDBStorage, ZSTD_COMPRESSION_LEVEL};
    use tempfile::TempDir;
    use crate::types::RocksDbConfig;
    use super::*;

    async fn setup() -> RocksDb {
        // Start and populate server
        let port = 9877;
        let v4_addr = SocketAddr::from(([127, 0, 0, 1], port));
        let addrs: &[std::net::SocketAddr] = &[v4_addr];
        let server = ServerBuilder::default().build(addrs).await.unwrap();

        let mut rpc_module = RpcModule::new(());

        let mut db_storage = initialize_storage();
        populate_with_test_data(&mut db_storage);
        let db = Arc::new(db_storage);
        let storage = Storage { storage: Arc::clone(&db) };

        rpc_module
            .merge(rocksdb_service::RocksDBServer::into_rpc(storage))
            .expect("RocksDBServer error");

        let _rpc_server_handle = server.start(rpc_module);

        // init client
        // TODO FIX test service above (tested that this client works when connecting to properly starting geyser-neon-filter)
        let rocksdb_url = format!("127.0.0.1:{}", port);
        tracing::info!("Opening client at {}", rocksdb_url);

        let config = RocksDbConfig { rocksdb_url };

        RocksDb::new(&config).await
    }

    fn initialize_storage() -> RocksDBStorage {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();

        RocksDBStorage::open(db_path, None, rocksdb_storage::storage::cf(), ZSTD_COMPRESSION_LEVEL).unwrap()
    }

    fn populate_with_test_data(storage: &mut RocksDBStorage) {
        (1..=10).for_each(|i| {
            storage
                .put_slot(&RootedSlot {
                    slot: i,
                    parent: None,
                })
                .unwrap()
        });
    }

    #[tokio::test]
    async fn test_get_last_rooted_slot() {
        let client = setup().await;
        let earliest_slot = client.get_earliest_rooted_slot().await.unwrap();
        tracing::info!("Earliest rooted slot {}", earliest_slot);
        assert_eq!(earliest_slot, 1);

        let last_slot = client.get_latest_block().await.unwrap();
        tracing::info!("Earliest rooted slot {}", last_slot);
        assert_eq!(last_slot, 10);
    }
}

