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

#[derive(Clone, Serialize)]
pub struct AccountParams {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::RocksDbConfig;

#[derive(Clone)]
pub struct RocksDb {
    pub url: String,
    pub client: Arc<WsClient>,
}

impl RocksDb {
    #[must_use]
    pub async fn new(config: &RocksDbConfig) -> Self {
        let addr = &config.rocksdb_url;
        let url = format!("ws://{addr}");

        match WsClientBuilder::default().build(&url).await {
            Ok(client) => {
                let arc_c = Arc::new(client);
                Self { url, client: arc_c }
            }
            Err(e) => panic!("Couln't start rocksDb client: {e}"),
        }
    }

    pub async fn get_block_time(&self, slot: Slot) -> RocksDbResult<UnixTimestamp> {
        let response: String = self
            .client
            .request("get_block_time", rpc_params![slot])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(i64::from_str(response.as_str())?)
    }
    pub async fn get_earliest_rooted_slot(&self) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_earliest_rooted_slot", rpc_params![])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_latest_block(&self) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_last_rooted_slot", rpc_params![])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> RocksDbResult<Option<Account>> {
        // let ap: AccountParams = AccountParams { pubkey: *pubkey, slot, tx_index_in_block };

        let response: String = self
            .client
            .request("get_account", rpc_params![pubkey, slot, tx_index_in_block])
            .await?;
        tracing::info!("response: {:?}", response);

        if let Some(account) = from_str(response.as_str())? {
            Ok(Some(account))
        } else {
            Ok(None)
        }
    }

    pub async fn get_transaction_index(&self, signature: Signature) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_transaction_index", rpc_params![signature])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_accounts(&self, start: u64, end: u64) -> RocksDbResult<Vec<Vec<u8>>> {
        let response: String = self
            .client
            .request("get_accounts", rpc_params![start, end])
            .await?;
        tracing::info!("response: {:?}", response);
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

    pub async fn get_slot_by_blockhash(&self, blockhash: &str) -> RocksDbResult<u64> {
        let response: String = self
            .client
            .request("get_slot_by_blockhash", rpc_params![blockhash])
            .await?;
        tracing::info!("response: {:?}", response);
        Ok(u64::from_str(response.as_str())?)
    }

    pub async fn get_sync_status(&self) -> RocksDbResult<EthSyncStatus> {
        Ok(EthSyncStatus::new(None))
    }
}

// #[cfg(test)]
// mod tests {
//     use std::net::SocketAddr;
//     use jsonrpsee::RpcModule;
//     use jsonrpsee::server::ServerBuilder;
//     use rocksdb_storage::rocksdb_structs::RootedSlot;
//     use rocksdb_storage::rpc::rocksdb_service;
//     use rocksdb_storage::rpc::rocksdb_service::Storage;
//     use rocksdb_storage::storage::{DBStorage, RocksDBStorage, ZSTD_COMPRESSION_LEVEL};
//     use tempfile::TempDir;
//     use crate::types::RocksDbConfig;
//     use super::*;
//
//     async fn setup() -> RocksDb {
//         // Start and populate server
//         let port = 9888;
//         let v4_addr = SocketAddr::from(([127, 0, 0, 1], port));
//         let addrs: &[std::net::SocketAddr] = &[v4_addr];
//         let server = ServerBuilder::default().build(addrs).await.unwrap();
//
//         let mut rpc_module = RpcModule::new(());
//
//         let mut db_storage = initialize_storage();
//         populate_with_test_data(&mut db_storage);
//         let db = Arc::new(db_storage);
//         let storage = Storage { storage: Arc::clone(&db) };
//
//         rpc_module
//             .merge(rocksdb_service::RocksDBServer::into_rpc(storage))
//             .expect("RocksDBServer error");
//
//         let _rpc_server_handle = server.start(rpc_module);
//
//         // init client
//         // TODO either FIX test service above or just rely on integration tests to test this, as this might be an overkill
//         let rocksdb_url = format!("127.0.0.1:{}", port);
//         tracing::info!("Opening client at {}", rocksdb_url);
//
//         let config = RocksDbConfig { rocksdb_url };
//
//         RocksDb::new(&config).await
//     }
//
//     fn initialize_storage() -> RocksDBStorage {
//         let temp_dir = TempDir::new().unwrap();
//         let db_path = temp_dir.path().to_str().unwrap();
//
//         RocksDBStorage::open(db_path, None, rocksdb_storage::storage::cf(), ZSTD_COMPRESSION_LEVEL).unwrap()
//     }
//
//     fn populate_with_test_data(storage: &mut RocksDBStorage) {
//         (1..=10).for_each(|i| {
//             storage
//                 .put_slot(&RootedSlot {
//                     slot: i,
//                     parent: None,
//                 })
//                 .unwrap()
//         });
//     }
//
//     #[tokio::test]
//     async fn test_get_last_rooted_slot() {
//         let client = setup().await;
//         let earliest_slot = client.get_earliest_rooted_slot().await.unwrap();
//         tracing::info!("Earliest rooted slot {}", earliest_slot);
//         assert_eq!(earliest_slot, 1);
//
//         let last_slot = client.get_latest_block().await.unwrap();
//         tracing::info!("Earliest rooted slot {}", last_slot);
//         assert_eq!(last_slot, 10);
//
//         let accounts = client.get_accounts(1, 12);
//         println!("ACCOUNTS: {}", accounts);
//     }
// }
