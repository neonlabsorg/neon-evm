use crate::{
    commands::get_neon_elf::get_elf_parameter,
    types::tracer_ch_common::{AccountRow, ChError, RevisionRow},
};

use super::tracer_ch_common::{ChResult, EthSyncStatus, EthSyncing, RevisionMap};

use crate::types::ChDbConfig;
use clickhouse::Client;
use log::{debug, error, info};
use rand::Rng;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::time::Instant;

#[derive(Clone)]
pub struct ClickHouseDb {
    pub client: Client,
}

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

        ClickHouseDb { client }
    }

    // return value is not used for tracer methods
    pub async fn get_block_time(&self, slot: Slot) -> ChResult<UnixTimestamp> {
        let time_start = Instant::now();
        let query =
            "SELECT JSONExtractInt(notify_block_json, 'block_time') FROM events.notify_block_distributed WHERE slot = ? LIMIT 1";
        let result = self
            .client
            .query(query)
            .bind(slot)
            .fetch_one::<UnixTimestamp>()
            .await
            .map_err(std::convert::Into::into);
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_block_time sql time: {} sec",
            execution_time.as_secs_f64()
        );
        result
    }

    pub async fn get_earliest_rooted_slot(&self) -> ChResult<u64> {
        let time_start = Instant::now();
        let query = "SELECT min(slot) FROM events.rooted_slots";
        let result = self
            .client
            .query(query)
            .fetch_one::<u64>()
            .await
            .map_err(std::convert::Into::into);
        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_earliest_rooted_slot sql returned {result:?}, time: {} sec",
            execution_time.as_secs_f64()
        );
        result
    }

    pub async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> ChResult<Option<Account>> {
        if let Some(tx_index_in_block) = tx_index_in_block {
            return if let Some(account) = self
                .get_account_at_index_in_block(pubkey, slot, tx_index_in_block)
                .await?
            {
                Ok(Some(account))
            } else {
                self.get_account_at_slot(pubkey, slot - 1).await
            };
        }

        self.get_account_at_slot(pubkey, slot).await
    }

    async fn get_account_at_slot(
        &self,
        pubkey: &Pubkey,
        slot: u64,
    ) -> Result<Option<Account>, ChError> {
        debug!("get_account_at_slot {{ pubkey: {pubkey}, slot: {slot} }}");

        let time_start = Instant::now();

        let query = r#"
            SELECT owner, lamports, executable, rent_epoch, data, txn_signature
            FROM events.update_account_distributed
            WHERE pubkey = ?
              AND slot <= ?
              AND (
                    SELECT COUNT(slot)
                    FROM events.rooted_slots
                    WHERE slot = ?
              ) >= 1
            ORDER BY slot DESC, write_version DESC
            LIMIT 1
        "#;

        let mut row = Self::row_opt(
            self.client
                .query(query)
                .bind(format!("{:?}", pubkey.to_bytes()))
                .bind(slot)
                .bind(slot)
                .fetch_one::<AccountRow>()
                .await,
        )
        .map_err(|e| {
            error!("get_account_at_slot error: {e}");
            ChError::Db(e)
        })?;

        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_account_at_slot {{ pubkey: {pubkey}, slot: {slot} }} returned {row:?}, time: {} sec",
            execution_time.as_secs_f64()
        );

        if row.is_none() {
            row = self.get_older_account_row_at(&pubkey, slot).await?;
        }

        let result = row
            .map(|a| a.try_into())
            .transpose()
            .map_err(|e| ChError::Db(clickhouse::error::Error::Custom(e)));

        debug!("get_account_at_slot {{ pubkey: {pubkey}, slot: {slot} }} -> {result:?}");

        result
    }

    async fn get_account_at_index_in_block(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: u64,
    ) -> ChResult<Option<Account>> {
        debug!(
            "get_account_at_index_in_block {{ pubkey: {pubkey}, slot: {slot}, tx_index_in_block: {tx_index_in_block} }}"
        );

        let query = r#"
            SELECT owner, lamports, executable, rent_epoch, data, txn_signature
            FROM events.update_account_distributed
            WHERE pubkey = ?
              AND slot = ?
              AND (
                    SELECT COUNT(slot)
                    FROM events.rooted_slots
                    WHERE slot = ?
              ) >= 1
              AND write_version <= ?
            ORDER BY write_version DESC
            LIMIT 1
        "#;

        let time_start = Instant::now();

        let account = Self::row_opt(
            self.client
                .query(query)
                .bind(format!("{:?}", pubkey.to_bytes()))
                .bind(slot)
                .bind(slot)
                .bind(tx_index_in_block)
                .fetch_one::<AccountRow>()
                .await,
        )
        .map_err(|e| {
            error!("get_account_at_index_in_block error: {e}");
            ChError::Db(e)
        })?
        .map(|a| a.try_into())
        .transpose()
        .map_err(|e| ChError::Db(clickhouse::error::Error::Custom(e)))?;

        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_account_at_index_in_block {{ pubkey: {pubkey}, slot: {slot}, tx_index_in_block: {tx_index_in_block} }} returned {account:?}, time: {} sec",
            execution_time.as_secs_f64()
        );

        Ok(account)
    }

    async fn get_older_account_row_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
    ) -> ChResult<Option<AccountRow>> {
        let time_start = Instant::now();

        let query = r#"
            SELECT owner, lamports, executable, rent_epoch, data, txn_signature
            FROM events.older_account_distributed FINAL
            WHERE pubkey = ? AND slot <= ? AND (
                SELECT COUNT(slot)
                FROM events.rooted_slots
                WHERE slot = ?
            ) >= 1
            ORDER BY slot DESC, write_version DESC
            LIMIT 1
        "#;
        let row = Self::row_opt(
            self.client
                .query(query)
                .bind(format!("{:?}", pubkey.to_bytes()))
                .bind(slot)
                .bind(slot)
                .fetch_one::<AccountRow>()
                .await,
        )
        .map_err(|e| {
            println!("get_last_older_account_row error: {e}");
            ChError::Db(e)
        })?;

        let execution_time = Instant::now().duration_since(time_start);
        info!(
            "get_older_account_row_at {{ pubkey: {pubkey}, slot: {slot} }} returned {row:?}, time: {} sec",
            execution_time.as_secs_f64()
        );

        Ok(row)
    }

    pub async fn get_neon_revision(&self, slot: Slot, pubkey: &Pubkey) -> ChResult<String> {
        let query = r#"SELECT data
        FROM events.update_account_distributed
        WHERE
            pubkey = ? AND slot <= ?
        ORDER BY
            pubkey ASC,
            slot ASC,
            write_version ASC
        LIMIT 1
        "#;

        let pubkey_str = format!("{:?}", pubkey.to_bytes());

        let data = Self::row_opt(
            self.client
                .query(query)
                .bind(pubkey_str)
                .bind(slot)
                .fetch_one::<Vec<u8>>()
                .await,
        )?;

        match data {
            Some(data) => {
                let neon_revision =
                    get_elf_parameter(data.as_slice(), "NEON_REVISION").map_err(|e| {
                        ChError::Db(clickhouse::error::Error::Custom(format!(
                            "Failed to get NEON_REVISION, error: {e:?}",
                        )))
                    })?;
                Ok(neon_revision)
            }
            None => {
                let err = clickhouse::error::Error::Custom(format!(
                    "get_neon_revision: for slot {slot} and pubkey {pubkey} not found",
                ));
                Err(ChError::Db(err))
            }
        }
    }

    pub async fn get_neon_revisions(&self, pubkey: &Pubkey) -> ChResult<RevisionMap> {
        let query = r#"SELECT slot, data
        FROM events.update_account_distributed
        WHERE
            pubkey = ?
        ORDER BY
            slot ASC,
            write_version ASC"#;

        let pubkey_str = format!("{:?}", pubkey.to_bytes());
        let rows: Vec<RevisionRow> = self
            .client
            .query(query)
            .bind(pubkey_str)
            .fetch_all()
            .await?;

        let mut results: Vec<(u64, String)> = Vec::new();

        for row in rows {
            let neon_revision = get_elf_parameter(&row.data, "NEON_REVISION").map_err(|e| {
                ChError::Db(clickhouse::error::Error::Custom(format!(
                    "Failed to get NEON_REVISION, error: {:?}",
                    e
                )))
            })?;
            results.push((row.slot, neon_revision));
        }
        let ranges = RevisionMap::build_ranges(results);

        Ok(RevisionMap::new(ranges))
    }

    pub async fn get_slot_by_blockhash(&self, blockhash: &str) -> ChResult<u64> {
        let query = r#"SELECT slot
        FROM events.notify_block_distributed
        WHERE hash = ?
        LIMIT 1
        "#;

        let slot = Self::row_opt(
            self.client
                .query(query)
                .bind(blockhash)
                .fetch_one::<u64>()
                .await,
        )?;

        match slot {
            Some(slot) => Ok(slot),
            None => Err(ChError::Db(clickhouse::error::Error::Custom(
                "get_slot_by_blockhash: no data available".to_string(),
            ))),
        }
    }

    pub async fn get_sync_status(&self) -> ChResult<EthSyncStatus> {
        let query_is_startup = r#"SELECT is_startup
        FROM events.update_account_distributed
        WHERE slot = (
          SELECT MAX(slot)
          FROM events.update_account_distributed
        )
        LIMIT 1
        "#;

        let is_startup = Self::row_opt(
            self.client
                .query(query_is_startup)
                .fetch_one::<bool>()
                .await,
        )?;

        if let Some(true) = is_startup {
            let query = r#"SELECT slot
            FROM (
              (SELECT MIN(slot) as slot FROM events.notify_block_distributed)
              UNION ALL
              (SELECT MAX(slot) as slot FROM events.notify_block_distributed)
              UNION ALL
              (SELECT MAX(slot) as slot FROM events.notify_block_distributed)
            )
            ORDER BY slot ASC
            "#;

            let data = Self::row_opt(self.client.query(query).fetch_one::<EthSyncing>().await)?;

            return match data {
                Some(data) => Ok(EthSyncStatus::new(Some(data))),
                None => Err(ChError::Db(clickhouse::error::Error::Custom(
                    "get_sync_status: no data available".to_string(),
                ))),
            };
        }

        Ok(EthSyncStatus::new(None))
    }

    fn row_opt<T>(result: clickhouse::error::Result<T>) -> clickhouse::error::Result<Option<T>> {
        match result {
            Ok(row) => Ok(Some(row)),
            Err(clickhouse::error::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
