use crate::NeonApiState;
use hex::FromHex;
use neon_lib::rpc::{CallDbClient, TrxDbClient};
use neon_lib::{context, rpc, NeonError};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// # Errors
pub fn build_rpc_client(state: &NeonApiState, slot: Option<u64>) -> Arc<dyn rpc::Rpc> {
    let config = &state.config;

    if let Some(slot) = slot {
        return build_call_db_client(state, slot);
    }

    Arc::new(RpcClient::new_with_commitment(
        config.json_rpc_url.clone(),
        config.commitment,
    ))
}

/// # Errors
pub fn build_call_db_client(state: &NeonApiState, slot: u64) -> Arc<dyn rpc::Rpc> {
    Arc::new(CallDbClient::new(state.tracer_db.clone(), slot))
}

/// # Errors
pub async fn build_hash_rpc_client(
    state: &NeonApiState,
    hash: &str,
) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
    let hash = <[u8; 32]>::from_hex(context::truncate_0x(hash))?;

    let db_config = state
        .config
        .db_config
        .as_ref()
        .expect("db-config not found");
    Ok(Arc::new(
        TrxDbClient::new(db_config, state.tracer_db.clone(), hash).await,
    ))
}
