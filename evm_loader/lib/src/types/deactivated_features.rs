// use crate::tracing::tracers::state_diff::Account;
use crate::rpc::Rpc;
use once_cell::sync::Lazy;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

static DEACTIVATED_FEATURES_UPDATE_SLOTS: u64 = 24 * 60 * 60; // approximately each 12h

#[derive(Debug, Eq, PartialEq, Clone, Default)]
struct DeactivatedFeaturesCache {
    // feature to slot when activated, if not then None
    pub data: HashMap<Pubkey, Option<u64>>,
    pub last_slot: u64,
}

impl DeactivatedFeaturesCache {
    fn should_update(&self, slot: u64) -> bool {
        self.last_slot >= DEACTIVATED_FEATURES_UPDATE_SLOTS + slot
    }
}

static DEACTIVATED_FEATURES: Lazy<Arc<Mutex<DeactivatedFeaturesCache>>> =
    Lazy::new(|| Arc::new(Mutex::new(DeactivatedFeaturesCache::default())));

async fn get_multiple_features(rpc: &dyn Rpc) -> ClientResult<HashMap<Pubkey, Option<u64>>> {
    let feature_keys: Vec<Pubkey> = solana_sdk::feature_set::FEATURE_NAMES
        .keys()
        .copied()
        .collect();

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

async fn deactivated_features_get_instance(
    rpc: &dyn Rpc,
) -> ClientResult<DeactivatedFeaturesCache> {
    let mut cache = DEACTIVATED_FEATURES.lock().await;

    let rpc_slot = rpc.get_slot().await?;
    if cache.should_update(rpc_slot) {
        cache.last_slot = rpc_slot;
        cache.data = get_multiple_features(rpc).await?;
    }

    Ok((*cache).clone())
}

pub async fn get_deactivated_features(
    rpc: &dyn Rpc,
    slot: Option<u64>,
) -> ClientResult<Vec<Pubkey>> {
    let features = deactivated_features_get_instance(rpc).await?;

    Ok(features
        .data
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
