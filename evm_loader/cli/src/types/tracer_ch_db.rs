use super::{block, ChDbConfig};
use clickhouse::{Client, Row};
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::{
    cmp::{
        Ord,
        Ordering::{Equal, Greater, Less},
    },
    convert::TryFrom,
    sync::Arc,
};
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

#[derive(Row, serde::Deserialize, Clone)]
pub struct SlotParent {
    pub slot: u64,
    pub parent: u64,
}

#[derive(Row, serde::Deserialize, Clone)]
pub struct AccountRow {
    owner: Vec<u8>,
    lamports: u64,
    executable: bool,
    rent_epoch: u64,
    data: Vec<u8>,
}

#[allow(dead_code)]
impl ClickHouseDb {
    pub fn new(config: &ChDbConfig) -> Self {
        let client = match (&config.clickhouse_user, &config.clickhouse_password) {
            (None, None | Some(_)) => Client::default().with_url(&config.clickhouse_url),
            (Some(user), None) => Client::default()
                .with_url(&config.clickhouse_url)
                .with_user(user),
            (Some(user), Some(password)) => Client::default()
                .with_url(&config.clickhouse_url)
                .with_user(user)
                .with_password(password),
        };

        ClickHouseDb {
            client: Arc::new(client),
        }
    }

    pub fn get_block_hash(&self, slot: u64) -> ChResult<String> {
        block(|| async {
            let query = "SELECT hash FROM events.notify_block_local where slot = ? ";
            self.client
                .query(query)
                .bind(slot)
                .fetch_one::<String>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    pub fn get_block_time(&self, slot: Slot) -> ChResult<UnixTimestamp> {
        block(|| async {
            let query =
                "SELECT JSONExtractInt(notify_block_json, 'block_time') FROM events.notify_block_local WHERE slot = ?)";
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

    pub fn get_latest_block(&self) -> ChResult<u64> {
        block(|| async {
            let query = "SELECT max(slot) FROM events.update_slot";
            self.client
                .query(query)
                .fetch_one::<u64>()
                .await
                .map_err(std::convert::Into::into)
        })
    }

    fn get_branch_slots(&self, slot: u64) -> ChResult<(u64, Vec<u64>)> {
        let rows: Vec<SlotParent> = block(|| async {
            let query = "SELECT distinct on (slot), ?fields FROM events.update_slot \
                WHERE slot >= (SELECT slot FROM events.update_slot WHERE status = 'Rooted' ORDER BY slot DESC LIMIT 1) \
                 and parent is not NULL \
                ORDER BY slot DESC, status DESC";
            self.client.query(query).fetch_all::<SlotParent>().await
        })?;

        let (root, rows) = rows.split_last().ok_or_else(|| {
            let err = clickhouse::error::Error::Custom("Rooted slot not found".to_string());
            ChError::Db(err)
        })?;

        match slot.cmp(&root.slot) {
            Less => {
                let count = block(|| async {
                    let query = "SELECT count(*) FROM events.update_slot WHERE slot = ? and status = 'Rooted'";
                    self.client.query(query).bind(slot).fetch_one::<u64>().await
                })?;

                if count == 0 {
                    let err = clickhouse::error::Error::Custom(format!(
                        "requested slot is not on working branch {}",
                        slot
                    ));
                    Err(ChError::Db(err))
                } else {
                    Ok((root.slot, vec![]))
                }
            }
            Equal => Ok((root.slot, vec![])),
            Greater => {
                let mut branch: Vec<SlotParent> = vec![];

                for row in rows {
                    if branch.is_empty() {
                        if row.slot == slot {
                            branch.push(row.clone());
                        }
                    } else if row.slot == branch.last().unwrap().parent {
                        branch.push(row.clone());
                    }
                }

                if branch.is_empty() {
                    let err = clickhouse::error::Error::Custom(format!(
                        "requested slot not found {}",
                        slot
                    ));
                    Err(ChError::Db(err))
                } else if branch.last().unwrap().parent == root.slot {
                    let branch = branch.iter().map(|row| row.slot).collect();
                    Ok((root.slot, branch))
                } else {
                    let err = clickhouse::error::Error::Custom(format!(
                        "requested slot is not on working branch {}",
                        slot
                    ));
                    Err(ChError::Db(err))
                }
            }
        }
    }

    pub fn get_account_at(&self, key: &Pubkey, slot: u64) -> ChResult<Option<Account>> {
        let (root, branch) = self.get_branch_slots(slot)?;
        let key_ = format!("{:?}", key.to_bytes());

        let mut row: Option<AccountRow> = if branch.is_empty() {
            None
        } else {
            let mut slots = format!("toUInt64({})", branch.first().unwrap());
            for slot in &branch[1..] {
                slots = format!("{}, toUInt64({})", slots, slot);
            }
            let result = block(|| async {
                let query = r#"
                SELECT owner, lamports, executable, rent_epoch, data
                FROM events.update_account_distributed
                WHERE
                    pubkey = ?
                    AND slot IN (SELECT arrayJoin([?]))
                ORDER BY slot DESC, pubkey DESC, write_version DESC
                LIMIT 1
                "#;
                self.client
                    .query(query)
                    .bind(key_.clone())
                    .bind(slots)
                    .fetch_one::<AccountRow>()
                    .await
            });

            match result {
                Ok(row) => Some(row),
                Err(clickhouse::error::Error::RowNotFound) => None,
                Err(e) => return Err(ChError::Db(e)),
            }
        };

        if row.is_none() {
            let result = block(|| async {
                let query = r#"
                SELECT owner, lamports, executable, rent_epoch, data
                FROM events.update_account_local
                WHERE
                    pubkey = ?
                    AND slot in (SELECT slot FROM events.update_slot WHERE status = 'Rooted' AND slot <= ?)
                ORDER BY slot DESC, pubkey DESC, write_version DESC
                LIMIT 1
                "#;
                self.client
                    .query(query)
                    .bind(key_.clone())
                    .bind(root)
                    .fetch_one::<AccountRow>()
                    .await
            });

            row = match result {
                Ok(row) => Some(row),
                Err(clickhouse::error::Error::RowNotFound) => None,
                Err(e) => return Err(ChError::Db(e)),
            };
        }

        if row.is_none() {
            let result = block(|| async {
                let query = r#"
                SELECT owner, lamports, executable, rent_epoch, data
                FROM events.older_account_distributed
                WHERE pubkey = ?
                ORDER BY slot DESC LIMIT 1
                "#;
                self.client
                    .query(query)
                    .bind(key_)
                    .fetch_one::<AccountRow>()
                    .await
            });

            row = match result {
                Ok(row) => Some(row),
                Err(clickhouse::error::Error::RowNotFound) => None,
                Err(e) => return Err(ChError::Db(e)),
            };
        }

        if let Some(acc) = row {
            let owner = Pubkey::try_from(acc.owner).map_err(|_| {
                let err = clickhouse::error::Error::Custom(format!(
                    "error convert owner of key: {}",
                    key
                ));
                ChError::Db(err)
            })?;

            Ok(Some(Account {
                lamports: acc.lamports,
                data: acc.data,
                owner,
                rent_epoch: acc.rent_epoch,
                executable: acc.executable,
            }))
        } else {
            Ok(None)
        }
    }

    #[allow(clippy::unused_self)]
    pub fn get_account_by_sol_sig(
        &self,
        _pubkey: &Pubkey,
        _sol_sig: &[u8; 64],
    ) -> ChResult<Option<Account>> {
        let err =
            clickhouse::error::Error::Custom("get_account_by_sol_sig not implemented".to_string());
        Err(ChError::Db(err))
    }
}
