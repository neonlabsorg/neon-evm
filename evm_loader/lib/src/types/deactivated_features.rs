use crate::rpc::Rpc;
use once_cell::sync::Lazy;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

static DEACTIVATED_FEATURES_PERIOD: Duration = Duration::from_secs(60 * 60); // 1 hour

pub trait BoxRpc: Rpc + Sync + Send {}
impl<T: Rpc + Sync + Send> BoxRpc for T {}

struct DeactivatedFeaturesCache {
    // feature to slot when activated, if not then None
    pub data: HashMap<Pubkey, Option<u64>>,
    pub last_update: Instant,
    pub rpc: Option<Box<dyn BoxRpc>>,
}

impl Default for DeactivatedFeaturesCache {
    fn default() -> Self {
        Self {
            data: HashMap::new(),
            last_update: Instant::now()
                .checked_sub(DEACTIVATED_FEATURES_PERIOD)
                .unwrap(),
            rpc: None,
        }
    }
}

impl DeactivatedFeaturesCache {
    pub fn should_update(&self) -> bool {
        if self.rpc.is_some() {
            self.last_update + DEACTIVATED_FEATURES_PERIOD <= Instant::now()
        } else {
            false
        }
    }

    pub fn set_deactivated_features_rpc(&mut self, rpc: impl BoxRpc + 'static) {
        self.rpc = Some(Box::new(rpc));
    }

    pub async fn update(&mut self) -> ClientResult<()> {
        if let Some(ref rpc) = self.rpc {
            self.last_update = Instant::now();
            self.data = get_multiple_features(rpc.as_ref()).await?;
        }

        Ok(())
    }
}

static DEACTIVATED_FEATURES: Lazy<Arc<Mutex<DeactivatedFeaturesCache>>> =
    Lazy::new(|| Arc::new(Mutex::new(DeactivatedFeaturesCache::default())));

async fn get_multiple_features(rpc: &dyn BoxRpc) -> ClientResult<HashMap<Pubkey, Option<u64>>> {
    let feature_keys: Vec<Pubkey> = agave_feature_set::FEATURE_NAMES.keys().copied().collect();

    let features = Rpc::get_multiple_accounts(rpc, &feature_keys).await?;

    let mut result = HashMap::<Pubkey, Option<u64>>::new();
    for (pubkey, feature) in feature_keys.iter().zip(features) {
        let slot = feature
            .and_then(|a| solana_sdk::feature::from_account(&a))
            .and_then(|f| f.activated_at);

        result.insert(*pubkey, slot);
    }

    Ok(result)
}

async fn get_deactivated_features() -> ClientResult<HashMap<Pubkey, Option<u64>>> {
    let mut cache = DEACTIVATED_FEATURES.lock().await;

    if cache.should_update() {
        cache.update().await?;
    };

    Ok(cache.data.clone())
}

pub async fn get_deactivated_features_at_slot(slot: Option<u64>) -> ClientResult<Vec<Pubkey>> {
    let features = get_deactivated_features().await?;

    Ok(features
        .into_iter()
        .filter_map(|(pubkey, activated_at)| match (slot, activated_at) {
            (_, None) => Some(pubkey),
            (Some(slot), Some(activated_at)) => {
                if activated_at > slot {
                    return Some(pubkey);
                }
                None
            }
            (None, Some(_)) => None,
        })
        .collect())
}

pub async fn set_deactivated_features_rpc(rpc: impl BoxRpc + 'static) {
    let mut cache = DEACTIVATED_FEATURES.lock().await;
    cache.set_deactivated_features_rpc(rpc);
}

#[cfg(test)]
#[path = "../deactivated_features_tests.rs"]
mod deactivated_features_tests;
