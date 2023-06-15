use super::{block, ChDbConfig};
use clickhouse::{Client, Row};
use log::info;
use rand::Rng;
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
    time::Instant,
};
use thiserror::Error;

const ROOT_BLOCK_DELAY: u8 = 100;

#[derive(Error, Debug)]
pub enum ChError {
    #[error("clickhouse: {}", .0)]
    Db(#[from] clickhouse::error::Error),
}

pub type ChResult<T> = std::result::Result<T, ChError>;

#[allow(dead_code)]
#[derive(Clone)]
pub struct ClickHouseDb {
    pub client: Arc<Client>,
}

#[derive(Row, serde::Deserialize, Clone)]
pub struct SlotParent {
    pub slot: u64,
    pub parent: Option<u64>,
    pub status: u8,
}

#[derive(Row, serde::Deserialize, Clone)]
pub struct AccountRow {
    owner: Vec<u8>,
    lamports: u64,
    executable: bool,
    rent_epoch: u64,
    data: Vec<u8>,
    write_version: Option<i64>,
    txn_signature: Option<Vec<u8>>,
}

impl TryInto<Account> for AccountRow {
    type Error = String;

    fn try_into(self) -> Result<Account, Self::Error> {
        let owner = Pubkey::try_from(self.owner).map_err(|src| {
            format!(
                "Incorrect slice length ({}) while converting owner from: {src:?}",
                src.len(),
            )
        })?;

        Ok(Account {
            lamports: self.lamports,
            data: self.data,
            owner,
            rent_epoch: self.rent_epoch,
            executable: self.executable,
        })
    }
}

#[allow(dead_code)]
impl ClickHouseDb {
    pub fn new(config: &ChDbConfig) -> Self {
        let url_id = rand::thread_rng().gen_range(0..config.clickhouse_url.len());
        let url = config.clickhouse_url.get(url_id).unwrap();

        let client = match (&config.clickhouse_user, &config.clickhouse_password) {
            (None, None | Some(_)) => Client::default().with_url(url),
            (Some(user), None) => Client::default().with_url(url).with_user(user),
            (Some(user), Some(password)) => Client::default()
                .with_url(url)
                .with_user(user)
                .with_password(password),
        };

        ClickHouseDb {
            client: Arc::new(client),
        }
    }

    // return value is not used for tracer methods
    pub fn get_block_time(&self, slot: Slot) -> ChResult<UnixTimestamp> {
        let time_start = Instant::now();
        let result = block(|| async {
            let query =
                "SELECT JSONExtractInt(notify_block_json, 'block_time') FROM events.notify_block_distributed WHERE slot = ? LIMIT 1";
            self.client
                .query(query)
                .bind(slot)
                .fetch_one::<UnixTimestamp>()
                .await
                .map_err(std::convert::Into::into)
        });
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_block_time sql time: {} sec",
            execution_time.as_secs_f64()
        );
        result
    }

    pub fn get_latest_block(&self) -> ChResult<u64> {
        let time_start = Instant::now();
        let result = block(|| async {
            let query = "SELECT max(slot) FROM events.update_slot";
            self.client
                .query(query)
                .fetch_one::<u64>()
                .await
                .map_err(std::convert::Into::into)
        });
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_latest_block sql time: {} sec",
            execution_time.as_secs_f64()
        );
        result
    }

    fn get_branch_slots(&self, slot: Option<u64>) -> ChResult<(u64, Vec<u64>)> {
        let query = r#"
            SELECT distinct on (slot) slot, parent, status FROM events.update_slot
            WHERE slot >= (
                  SELECT slot - ? FROM events.update_slot
                  WHERE status = 'Rooted'
                  ORDER BY slot DESC LIMIT 1
              )
              AND isNotNull(parent)
            ORDER BY slot DESC, status DESC
            "#;
        let time_start = Instant::now();
        let rows = block(|| async {
            self.client
                .query(query)
                .bind(ROOT_BLOCK_DELAY)
                .fetch_all::<SlotParent>()
                .await
        })?;

        let (last, rows) = rows.split_last().ok_or_else(|| {
            let err = clickhouse::error::Error::Custom("Rooted slot not found".to_string());
            ChError::Db(err)
        })?;

        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_branch_slot sql(1) time: {} sec",
            execution_time.as_secs_f64()
        );

        if slot.is_none() {
           let slot = last;
        }

        match slot.cmp(&last.slot) {
            Less | Equal => Ok((slot, vec![])),
            Greater => {
                let mut branch: Vec<SlotParent> = vec![];

                for row in rows {
                    if branch.is_empty() {
                        if row.slot == slot {
                            branch.push(row.clone());
                        }
                    } else if row.slot == branch.last().unwrap().parent.unwrap() {
                        branch.push(row.clone());
                    }
                }

                if branch.is_empty() {
                    let err = clickhouse::error::Error::Custom(format!(
                        "requested slot not found {slot}",
                    ));
                    Err(ChError::Db(err))
                } else {
                    let branch = branch.iter().map(|row| row.slot).collect();
                    Ok((last.slot, branch))
                }
            }
        }
    }

    fn get_account_rooted_slots(&self, key: &str, slot: u64) -> ChResult<Vec<u64>> {
        let query = r#"
            SELECT b.slot
            FROM events.update_slot AS b
            WHERE (b.slot IN (
                      SELECT a.slot
                      FROM events.update_account_distributed AS a
                      WHERE (a.pubkey = ?)
                        AND (a.slot <= ?)
                      ORDER BY a.pubkey, a.slot DESC
                      LIMIT 1000))
              AND (b.status = 'Rooted')
            ORDER BY b.slot DESC
            LIMIT 1
        "#;

        let time_start = Instant::now();
        let rows = block(|| async {
            self.client
                .query(query)
                .bind(key)
                .bind(slot)
                .fetch_all::<u64>()
                .await
        })?;

        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_account_rooted_slots sql(1) time: {} sec",
            execution_time.as_secs_f64()
        );

        Ok(rows)
    }

    #[allow(clippy::too_many_lines)]
    pub fn get_account_at(&self, pubkey: &Pubkey, slot: u64) -> ChResult<Option<Account>> {
        let (last, mut branch) = self.get_branch_slots(slot).map_err(|e| {
            println!("get_branch_slots error: {:?}", e);
            e
        })?;

        let pubkey_str = format!("{:?}", pubkey.to_bytes());

        let mut rooted_slots = self
            .get_account_rooted_slots(&pubkey_str, last)
            .map_err(|e| {
                println!("get_account_rooted_slots error: {:?}", e);
                e
            })?;
        branch.append(rooted_slots.as_mut());

        let mut row: Option<AccountRow> = if branch.is_empty() {
            None
        } else {
            let query = r#"
                SELECT owner, lamports, executable, rent_epoch, data
                FROM events.update_account_distributed
                WHERE pubkey = ?
                  AND slot IN ?
                ORDER BY pubkey, slot DESC, write_version DESC
                LIMIT 1
            "#;

            let time_start = Instant::now();
            let result = block(|| async {
                self.client
                    .query(query)
                    .bind(pubkey_str.clone())
                    .bind(&branch.as_slice())
                    .fetch_one::<AccountRow>()
                    .await
            });
            let execution_time = Instant::now().duration_since(time_start);
            info!(
                "get_account_at sql(1) time: {} sec",
                execution_time.as_secs_f64()
            );

            Self::row_opt(result).map_err(|e| {
                println!("get_account_at error: {e}");
                ChError::Db(e)
            })?
        };

        if row.is_none() {
            let time_start = Instant::now();
            row = block(|| self.get_last_older_account_row(&pubkey_str))?;
            let execution_time = Instant::now().duration_since(time_start);
            info!(
                "get_account_at sql(3) time: {} sec",
                execution_time.as_secs_f64()
            );
        }

        if let Some(acc) = row {
            acc.try_into()
                .map(Some)
                .map_err(|err| ChError::Db(clickhouse::error::Error::Custom(err)))
        } else {
            Ok(None)
        }
    }

    async fn get_last_older_account_row(&self, pubkey: &str) -> ChResult<Option<AccountRow>> {
        let query = r#"
            SELECT owner, lamports, executable, rent_epoch, data
            FROM events.older_account_distributed
            WHERE pubkey = ?
            ORDER BY slot DESC
            LIMIT 1
        "#;
        Self::row_opt(
            self.client
                .query(query)
                .bind(pubkey)
                .fetch_one::<AccountRow>()
                .await,
        )
        .map_err(|e| {
            println!("get_last_older_account_row error: {e}");
            ChError::Db(e)
        })
    }

    async fn get_sol_sig_rooted_slot(&self, sol_sig: &[u8; 64]) -> ChResult<Option<u64>> {
        let query = r#"
            SELECT b.slot
            FROM events.update_slot AS b
            WHERE (b.slot IN (
                      SELECT a.slot
                      FROM events.notify_transaction_distributed AS a
                      WHERE (a.signature = ?)
                  ))
            AND (b.status = 'Rooted')
            ORDER BY b.slot DESC
            LIMIT 1
        "#;

        Self::row_opt(block(|| async {
            self.client
                .query(query)
                .bind(sol_sig.as_slice())
                .fetch_one::<u64>()
                .await
        }))
        .map_err(|e| {
            println!("get_sol_sig_rooted_slot error: {e}");
            ChError::Db(e)
        })
    }

    async fn get_sol_sig_confirmed_slot(&self, sol_sig: &[u8; 64]) -> ChResult<Option<u64>> {
        let (last, slot_vec) = self.get_branch_slots(None);
        let query = r#"
            SELECT b.slot
            FROM events.update_slot AS b
            WHERE (b.slot IN ?)
            AND (b.slot IN (
                  SELECT a2.slot
                  FROM events.notify_transaction_distributed AS a2
                  WHERE (a2.signature = ?)
              ))
            ORDER BY b.slot DESC
            LIMIT 1
        "#;

        Self::row_opt(block(|| async {
            self.client
                .query(query)
                .bind(&slot_vec.as_slice())
                .bind(sol_sig.as_slice())
                .fetch_one::<u64>()
                .await
        }))
        .map_err(|e| {
            println!("get_sol_sig_confirmed_slot error: {e}");
            ChError::Db(e)
        })
    }

    #[allow(clippy::unused_self)]
    pub fn get_account_by_sol_sig(
        &self,
        pubkey: &Pubkey,
        sol_sig: &[u8; 64],
    ) -> ChResult<Option<Account>> {
        let time_start = Instant::now();
        let mut row = block(|| self.get_sol_sig_rooted_slot(&sol_sig))?;
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_sol_sig_rooted_slot sql(1) time: {} sec",
            execution_time.as_secs_f64()
        );

        if row.is_none() {
            let time_start = Instant::now();
            row = block(|| self.get_sol_sig_confirmed_slot(&sol_sig))?;
            let execution_time = Instant::now().duration_since(time_start);
            info!(
                "get_sol_sig_confirmed_slot sql(2) time: {} sec",
                execution_time.as_secs_f64()
            );
        }

        let Some(slot) = row else {
            return Ok(None)
        };

        // Check, if have records without `txn_signature` or with `write_version` < 0
        // Also try to find right `write_version`. If found and all checks are OK, return account.
        let query = r#"
            SELECT DISTINCT ON (pubkey, txn_signature, write_version)
                   owner, lamports, executable, rent_epoch, data, write_version, txn_signature
            FROM events.update_account_distributed
            WHERE slot = ? AND pubkey = ?
            ORDER BY write_version DESC
        "#;

        let pubkey_str = format!("{:?}", pubkey.to_bytes());
        let time_start = Instant::now();
        let rows = block(|| async {
            self.client
                .query(query)
                .bind(slot)
                .bind(pubkey_str.clone())
                .fetch_all::<AccountRow>()
                .await
        })?;
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_account_by_sol_sig sql(3) time: {} sec",
            execution_time.as_secs_f64()
        );

        let mut row_found = None;
        let mut found_signature = false;
        for row in rows {
            let (Some(write_version), Some(sig)) = (&row.write_version, &row.txn_signature) else {
                info!("get_sol_sig_confirmed_slot time cannot extract (write_version, txn_signature)!");
                return Ok(None);
            };
            // rent payment -> no changes of the record in the block
            if *write_version < 0 {
                row_found = Some(row);
                break;
            }
            if sig.as_slice() == sol_sig.as_slice() {
                found_signature = true;
                continue;
            }
            if found_signature {
                row_found = Some(row);
                break;
            }
        }

        // If not found, get closest account state in one of previous slots
        if row_found.is_some() {
            return row_found
                .map(|row| {
                    row.try_into()
                        .map_err(|err| ChError::Db(clickhouse::error::Error::Custom(err)))
                })
                .transpose()
        }

        self.get_account_at(pubkey, slot)
    }

    fn row_opt<T>(result: clickhouse::error::Result<T>) -> clickhouse::error::Result<Option<T>> {
        match result {
            Ok(row) => Ok(Some(row)),
            Err(clickhouse::error::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
