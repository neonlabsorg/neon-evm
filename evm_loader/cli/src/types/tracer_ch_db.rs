use super::block;
use clickhouse::{Client, Row};
use serde::{Deserialize, Serialize};
use solana_sdk::clock::Epoch;
use solana_sdk::clock::{Slot, UnixTimestamp};
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChError {
    #[error("clickhouse: {}", .0)]
    Db(#[from] clickhouse::error::Error),
}

pub type ChResult<T> = std::result::Result<T, ChError>;

pub struct ClickHouseDb {
    client: Arc<Client>,
}

#[derive(Debug, Row, Serialize, Deserialize)]
pub struct DbAccount {
    /// lamports in the account
    pub lamports: u64,
    /// data held in this account
    pub data: Vec<u8>,
    /// the program that owns this account. If executable, the program that loads this account.
    pub owner: Pubkey,
    /// this account's data contains a loaded program (and is now read-only)
    pub executable: bool,
    /// the epoch at which this account will next owe rent
    pub rent_epoch: Epoch,
}

impl From<DbAccount> for Account {
    fn from(db_account: DbAccount) -> Self {
        Account {
            lamports: db_account.lamports,
            data: db_account.data,
            owner: db_account.owner,
            executable: db_account.executable,
            rent_epoch: db_account.rent_epoch,
        }
    }
}

#[allow(dead_code)]
impl ClickHouseDb {
    pub fn _new(server_url: &str, username: Option<&str>, password: Option<&str>) -> ClickHouseDb {
        let client = match (username, password) {
            (None, None | Some(_)) => Client::default().with_url(server_url),
            (Some(user), None) => Client::default().with_url(server_url).with_user(user),
            (Some(user), Some(password)) => Client::default()
                .with_url(server_url)
                .with_user(user)
                .with_password(password),
        };

        ClickHouseDb {
            client: Arc::new(client),
        }
    }

    pub fn get_block_time(&self, slot: Slot) -> ChResult<UnixTimestamp> {
        block(|| async {
            let query = "SELECT JSONExtractInt(notify_block_json, 'block_time') FROM events.notify_block_local WHERE slot = ?";
            self.client
                .query(query)
                .bind(slot)
                .fetch_one::<UnixTimestamp>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    pub fn get_latest_blockhash(&self) -> ChResult<String> {
        block(|| async {
            let query =
                "SELECT hash FROM events.notify_block_distributed ORDER BY retrieved_time DESC LIMIT 1";
            self.client
                .query(query)
                .fetch_one::<String>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    pub fn get_latest_block(&self) -> ChResult<u64> {
        block(|| async {
            let query = "SELECT MAX(slot) FROM events.notify_block_distributed";
            self.client
                .query(query)
                .fetch_one::<u64>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    pub fn get_block_hash(&self, slot: u64) -> ChResult<String> {
        block(|| async {
            let query = "SELECT hash FROM events.notify_block_distributed WHERE slot = ?";
            self.client
                .query(query)
                .bind(slot)
                .fetch_one::<String>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    pub fn get_account_at(&self, pubkey: &Pubkey, slot: u64) -> ChResult<Option<Account>> {
        block(|| async {
            let query = "SELECT lamports, data, owner, executable, rent_epoch FROM events.update_account_distributed
            WHERE pubkey = ?
            AND slot <= ?
            ORDER BY write_version DESC
            LIMIT 1";
            match self
                .client
                .query(query)
                .bind(pubkey)
                .bind(slot)
                .fetch_one::<DbAccount>()
                .await
            {
                Ok(account) => Ok(Some(account.into())),
                Err(e) => Err(ChError::Db(e)),
            }
        })
    }

    pub fn get_account_by_sol_sig(
        &self,
        _pubkey: &Pubkey,
        _sol_sig: &[u8; 64],
    ) -> ChResult<Option<Account>> {
        let _ = self;
        unimplemented!()
    }
}
