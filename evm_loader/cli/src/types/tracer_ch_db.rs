use super::block;
use clickhouse::{Client, Row};
use solana_sdk::clock::{Slot, UnixTimestamp};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChError {
    #[error("clickhouse: {}", .0)]
    Db(#[from] clickhouse::error::Error),
    // #[error("Custom: {0}")]
    // Custom (String),
}

pub type ChResult<T> = std::result::Result<T, ChError>;

#[allow(dead_code)]
pub struct ClickHouseDb {
    client: Arc<Client>,
}

#[derive(Row, serde::Deserialize)]
pub struct SlotParent{
    pub slot: u64,
    pub parent: Option<u64>,
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
            let query = "SELECT JSONExtractInt(notify_block_json, 'block_time') FROM events.notify_block_local WHERE (slot = toUInt64(?))";
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
                "SELECT hash FROM events.notify_block_local ORDER BY retrieved_time DESC LIMIT 1";
            self.client
                .query(query)
                .fetch_one::<String>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    fn get_branch_slots(&self, slot: Slot) -> ChResult<Vec<u64>>{
        let max_rooted_slot: u64 = block(|| async {
            let query = "SELECT max(slot) from events.update_slot_local WHERE slot_status = 2";
            self.client
                .query(query)
                .fetch_one::<u64>()
                .await
        })?;

        let rows: Vec<SlotParent> =  block(|| async {
            let query = "SELECT distinct ?fields FROM events.update_slot_distributed \
                WHERE slot > ?\
                ORDER BY slot desc";
            self.client
                .query(query)
                .bind(max_rooted_slot)
                .fetch_all::<SlotParent>()
                .await
        })?;

        let mut branch: Vec<u64> = vec![];
        let mut parent = 0;
        let mut found_branch = false;

        for row in rows {
            if !found_branch {
                if row.slot == slot {
                    if let Some(parent_) = row.parent {
                        parent = parent_;
                        branch.push(row.slot);
                        found_branch = true;
                    }
                }
            } else {
                if row.slot == parent {
                    if let Some(parent_) = row.parent {
                        branch.push(row.slot);
                        parent = parent_;
                    }
                }
            }
        }
        if parent == max_rooted_slot {
            Ok(branch)
        } else {
            let err = clickhouse::error::Error::Custom(format!("requested slot is not on working branch {}", slot));
            Err(ChError::Db(err))
        }
    }
}
