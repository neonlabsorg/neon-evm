use rocksdb_storage::storage::RocksDBStorage;
use tempfile::TempDir;
// use log::{debug, error, info};

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
use thiserror::Error;

#[derive(Clone)]
pub struct RocksDb {
    // TODO: testing locally for now and will replace with RPC Client sending requests to RocksDB Server
    pub storage : RocksDBStorage,
}

impl RocksDb {
    #[must_use]
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();

        // TODO: default initialization
        let storage = RocksDBStorage::initialize(db_path);

        Self { storage }
    }

    pub async fn get_block_time(&self, slot: Slot) -> RocksDbResult<UnixTimestamp> {
        self.storage.get_block(slot).unwrap()?.block_time.unwrap()?
    }

    pub async fn get_earliest_rooted_slot(&self) -> RocksDbResult<u64> {
        self.storage.get_earliest_rooted_slot()
    }

    pub async fn get_latest_block(&self) -> RocksDbResult<u64> {
        self.storage.get_latest_slot()
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> RocksDbResult<Option<Account>> {
        if let Some(tx_index_in_block) = tx_index_in_block {
            return if let Some(account) = self
                .storage
                .get_account(pubkey.clone().as_ref(), slot, tx_index_in_block as i64).unwrap()
            {
                Ok(Some(try_from(account)).unwrap())
            } else {
                self
                    .storage
                    //  TODO confirm these parameters
                    .get_account_by_pubkey_and_slot_closest(pubkey.as_ref(), slot - 1, 2)
                    .unwrap()
            };
        }

        self
            .storage
            //  TODO confirm these parameters
            .get_account_by_pubkey_and_slot_closest(pubkey.as_ref(), slot, 2)
    }
}

