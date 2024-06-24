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
        // let ap: AccountParams = AccountParams { pubkey: *pubkey, slot, tx_index_in_block };

        let response: String = self
            .client
            .request(
                "get_account",
                rpc_params![pubkey.to_owned(), slot, tx_index_in_block],
            )
            .await?;
        tracing::info!("get_account_at response: {:?}", response);

        if let Some(account) = from_str(response.as_str())? {
            Ok(Some(account))
        } else {
            Ok(None)
        }
    }

    pub async fn get_transaction_index(&self, signature: Signature) -> RocksDbResult<u64> {
        let signature_str = format!("{:?}", signature);
        let response: String = self
            .client
            .request("get_transaction_index", rpc_params![signature_str])
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

    pub async fn get_slot_by_blockhash(&self, blockhash: String) -> RocksDbResult<u64> {
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
//     use super::*;
//     use crate::types::RocksDbConfig;
//     use solana_sdk::signature::Signature;
//     // use jsonrpsee::server::ServerBuilder;
//     // use jsonrpsee::RpcModule;
//     // use rocksdb_storage::rocksdb_structs::RootedSlot;
//     // use rocksdb_storage::rpc::rocksdb_service;
//     // use rocksdb_storage::rpc::rocksdb_service::Storage;
//     // use rocksdb_storage::storage::{DBStorage, RocksDBStorage, ZSTD_COMPRESSION_LEVEL};
//     // use std::net::SocketAddr;
//     // use tempfile::TempDir;
//
//     async fn setup() -> RocksDb {
//         // Start and populate server
//         // let rocksdb_port: u16 = 9888;
//         // let v4_addr = SocketAddr::from(([127, 0, 0, 1], rocksdb_port));
//         // let addrs: &[std::net::SocketAddr] = &[v4_addr];
//         // let server = ServerBuilder::default()
//         //     .ws_only()
//         //     .build(addrs)
//         //     .await
//         //     .unwrap();
//         //
//         // let mut rpc_module = RpcModule::new(());
//         //
//         // let mut db_storage = initialize_storage();
//         // populate_with_test_data(&mut db_storage);
//         // let db = Arc::new(db_storage);
//         // let storage = Storage {
//         //     storage: Arc::clone(&db),
//         // };
//         //
//         // rpc_module
//         //     .merge(rocksdb_service::RocksDBServer::into_rpc(storage))
//         //     .expect("RocksDBServer error");
//         ////
//         // let _rpc_server_handle = server.start(rpc_module);
//
//         // init client
//         // TODO either FIX test service above or just rely on integration tests to test this, as this might be an overkill
//         let rocksdb_host = "127.0.0.1".to_owned();
//         let rocksdb_port = 9888;
//         tracing::info!("Opening client at {rocksdb_host}:{rocksdb_port}");
//
//         let config = RocksDbConfig {
//             rocksdb_host,
//             rocksdb_port,
//         };
//
//         RocksDb::new(&config).await
//     }
//
//     // fn initialize_storage() -> RocksDBStorage {
//     //     let temp_dir = TempDir::new().unwrap();
//     //     let db_path = temp_dir.path().to_str().unwrap();
//     //
//     //     RocksDBStorage::open(
//     //         db_path,
//     //         None,
//     //         rocksdb_storage::storage::cf(),
//     //         ZSTD_COMPRESSION_LEVEL,
//     //     )
//     //     .unwrap()
//     // }
//
//     // fn populate_with_test_data(storage: &mut RocksDBStorage) {
//     //     (1..=10).for_each(|i| {
//     //         storage
//     //             .put_slot(&RootedSlot {
//     //                 slot: i,
//     //                 parent: None,
//     //             })
//     //             .unwrap()
//     //     });
//     // }
//
//     #[tokio::test]
//         async fn test_get_last_rooted_slot() {
//             let client = setup().await;
//
//             let earliest_slot = client.get_earliest_rooted_slot().await.unwrap();
//             println!("Earliest rooted slot {}", earliest_slot);
//             assert_eq!(earliest_slot, 1);
//
//             let last_slot = client.get_latest_block().await.unwrap();
//             println!("Latest block {}", last_slot);
//             // assert_eq!(last_slot, 10);
//
//             let accounts = client.get_accounts(1, 12).await.unwrap();
//             for acc in accounts {
//                 println!("ACCOUNT: {:?}: {:?}", acc.len(), acc);
//             }
//         }
//
//     #[tokio::test]
//     async fn test_get_slot_by_blockhash() {
//         let client = setup().await;
//         // let block = client.get_block(8).await.unwrap();
//         let slot8 = client.get_slot_by_blockhash("8HfsUf5H5RcZGENqEfFccrBCtWYC6uGYAQvNkZiEHqCU".to_string()).await.unwrap();
//         assert_eq!(slot8, 8);
//     }
//
//     #[tokio::test]
//     async fn test_get_transaction_index() {
//         let client = setup().await;
//         let signature_str = "5B4wxum51mVN2rp9XQMvF7yUErJhsMTtAyjKbzx4thASxqJFpZgJjkqZ36VKAMa1vnvKwRNsCSo2WnA9qWrmiQHW".to_string();
//         let signature = Signature::from_str(&signature_str).unwrap();
//         let index = client.get_transaction_index(signature).await.unwrap();
//         assert_eq!(index, 101);
//     }
// }
