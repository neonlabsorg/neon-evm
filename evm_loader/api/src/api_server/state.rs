use crate::Config;
use neon_lib::rpc::CallDbClient;
use neon_lib::types::TracerDb;
use neon_lib::{rpc, NeonError};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

pub struct State {
    pub tracer_db: TracerDb,
    pub rpc_client: Arc<RpcClient>,
    pub config: Config,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self {
            tracer_db: TracerDb::new(&config),
            rpc_client: Arc::new(config.build_solana_rpc_client()),
            config,
        }
    }

    pub async fn build_rpc(
        &self,
        slot: Option<u64>,
        tx_index_in_block: Option<u64>,
    ) -> Result<Arc<dyn rpc::Rpc>, NeonError> {
        Ok(if let Some(slot) = slot {
            Arc::new(CallDbClient::new(self.tracer_db.clone(), slot, tx_index_in_block).await?)
        } else {
            self.rpc_client.clone()
        })
    }
}
