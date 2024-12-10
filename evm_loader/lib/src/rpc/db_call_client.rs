use std::collections::HashMap;

use super::{e, Rpc, SliceConfig};
use crate::types::{TracerDb, TracerDbTrait};
use crate::NeonError;
use crate::NeonError::RocksDb;
use async_trait::async_trait;
use log::debug;
use solana_client::{
    client_error::Result as ClientResult,
    client_error::{ClientError, ClientErrorKind},
};
use solana_sdk::{account::Account, pubkey::Pubkey};

pub struct CallDbClient {
    tracer_db: TracerDb,
    slot: u64,
    tx_index_in_block: Option<u64>,
}

impl CallDbClient {
    pub async fn new(
        tracer_db: TracerDb,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> Result<Self, NeonError> {
        let earliest_rooted_slot = tracer_db
            .get_earliest_rooted_slot()
            .await
            .map_err(RocksDb)?;

        if slot < earliest_rooted_slot {
            return Err(NeonError::EarlySlot(slot, earliest_rooted_slot));
        }

        Ok(Self {
            tracer_db,
            slot,
            tx_index_in_block,
        })
    }

    async fn get_account_at(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        self.tracer_db
            .get_account_at(key, self.slot, self.tx_index_in_block, slice)
            .await
            .map_err(|e| e!("load account error", key, e))
    }
}

#[async_trait(?Send)]
impl Rpc for CallDbClient {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        self.get_account_at(key, slice).await
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        let mut result = Vec::new();
        for key in pubkeys {
            result.push(self.get_account_at(key, None).await?);
        }
        debug!("get_multiple_accounts: pubkeys={pubkeys:?} result={result:?}");
        Ok(result)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        use std::time::{Duration, Instant};
        use tokio::sync::Mutex;

        struct Cache {
            // feature to slot when activated, if not then None
            data: HashMap<Pubkey, Option<u64>>,
            timestamp: Instant,
        }

        static CACHE: Mutex<Option<Cache>> = Mutex::const_new(None);
        let mut cache = CACHE.lock().await;

        if let Some(cache) = cache.as_ref() {
            if cache.timestamp.elapsed() < Duration::from_secs(24 * 60 * 60) {
                let mut keys: Vec<Pubkey> = cache.data.keys().copied().collect();

                keys.retain(|pubkey| {
                    let value = cache.data.get(pubkey);

                    if let Some(Some(slot)) = value {
                        if slot <= &self.slot {
                            return false;
                        }
                    }

                    true
                });

                return Ok(keys);
            }
        }

        let feature_keys: Vec<Pubkey> = solana_sdk::feature_set::FEATURE_NAMES
            .keys()
            .copied()
            .collect();

        let tracer_db = self.tracer_db.clone();
        let slot = tracer_db
            .get_latest_block()
            .await
            .map_err(|e| e!("get_latest_block error", e))?;

        let self_rpc: Self = Self {
            tracer_db,
            slot,
            tx_index_in_block: None,
        };

        let features = Rpc::get_multiple_accounts(&self_rpc, &feature_keys).await?;

        let mut result = HashMap::<Pubkey, Option<u64>>::new();
        for (pubkey, feature) in feature_keys.iter().zip(features) {
            let slot = feature
                .and_then(|a| solana_sdk::feature::from_account(&a))
                .and_then(|f| f.activated_at);

            result.insert(*pubkey, slot);
        }

        cache.replace(Cache {
            data: result.clone(),
            timestamp: Instant::now(),
        });
        drop(cache);

        Ok(result
            .into_iter()
            .filter_map(|(pubkey, slot)| {
                if slot.is_none() {
                    return Some(pubkey);
                }
                None
            })
            .collect())
    }
}
