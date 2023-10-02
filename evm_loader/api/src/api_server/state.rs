use crate::Config;
use neon_lib::rpc::CallDbClient;
use neon_lib::types::TracerDb;
use neon_lib::{rpc, NeonError, RequestContext};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

pub struct State {
    tracer_db: TracerDb,
    rpc_client: Arc<RpcClient>,
    config: Config,
}

impl State {
    pub fn new(config: Config) -> Self {
        let db_config = config.db_config.as_ref().expect("db-config not found");
        Self {
            tracer_db: TracerDb::new(db_config),
            rpc_client: Arc::new(RpcClient::new_with_commitment(
                config.json_rpc_url.clone(),
                config.commitment,
            )),
            config,
        }
    }

    async fn build_rpc_client(
        &self,
        slot: Option<u64>,
        tx_index_in_block: Option<u64>,
    ) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
        if let Some(slot) = slot {
            return Ok(Arc::new(
                CallDbClient::new(self.tracer_db.clone(), slot, tx_index_in_block).await?,
            ));
        }

        Ok(self.rpc_client.clone())
    }

    pub async fn request_context(
        &self,
        slot: Option<u64>,
        tx_index_in_block: Option<u64>,
    ) -> Result<RequestContext, NeonError> {
        RequestContext::new(
            self.build_rpc_client(slot, tx_index_in_block).await?,
            &self.config,
        )
    }
}
