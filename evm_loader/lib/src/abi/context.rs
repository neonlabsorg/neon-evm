use solana_client::nonblocking::rpc_client::RpcClient;

use crate::{
    rpc::{self, CallDbClient},
    types::TracerDb,
    Config, NeonError,
};
use std::sync::Arc;

pub struct AbiContext {
    pub tracer_db: TracerDb,
    pub rpc_client: Arc<dyn rpc::Rpc + Send + Sync>,
    pub config: Config,
}

impl AbiContext {
    pub fn new(config: Config) -> Result<Self, NeonError> {
        let db_config = config
            .db_config
            .as_ref()
            .ok_or(NeonError::LoadingDBConfigError)?;
        Ok(Self {
            tracer_db: TracerDb::new(db_config),
            rpc_client: Arc::new(RpcClient::new_with_commitment(
                config.json_rpc_url.clone(),
                config.commitment,
            )),
            config,
        })
    }
}

pub async fn build_rpc_client(
    context: &AbiContext,
    slot: Option<u64>,
    tx_index_in_block: Option<u64>,
) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
    if let Some(slot) = slot {
        return build_call_db_client(context, slot, tx_index_in_block).await;
    }

    Ok(context.rpc_client.clone())
}

pub async fn build_call_db_client(
    context: &AbiContext,
    slot: u64,
    tx_index_in_block: Option<u64>,
) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
    Ok(Arc::new(
        CallDbClient::new(context.tracer_db.clone(), slot, tx_index_in_block).await?,
    ))
}
