use crate::NeonApiState;
use neon_lib::rpc::CallDbClient;
use neon_lib::{rpc, NeonError};
use std::sync::Arc;

pub async fn build_rpc_client(
    state: &NeonApiState,
    slot: Option<u64>,
    write_version: Option<u64>,
) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
    if let Some(slot) = slot {
        build_call_db_client(state, slot, write_version).await
    } else {
        Ok(state.rpc_client.clone())
    }
}

pub async fn build_call_db_client(
    state: &NeonApiState,
    slot: u64,
    write_version: Option<u64>,
) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
    Ok(Arc::new(
        CallDbClient::new(state.tracer_db.clone(), slot, write_version).await?,
    ))
}
