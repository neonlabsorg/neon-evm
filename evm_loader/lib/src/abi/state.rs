use crate::config::APIOptions;
use crate::rpc::{CallDbClient, CloneRpcClient, RpcEnum};
use crate::types::TracerDbType;
use crate::NeonError;

pub struct State {
    pub tracer_db: TracerDbType,
    pub rpc_client: CloneRpcClient,
    pub config: APIOptions,
}

impl State {
    #[must_use]
    pub async fn new(config: APIOptions) -> Self {
        let tracer_db = TracerDbType::from_config(&config.db_config).await;
        Self {
            tracer_db,
            rpc_client: CloneRpcClient::new_from_api_config(&config),
            config,
        }
    }

    pub async fn build_rpc(
        &self,
        slot: Option<u64>,
        tx_index_in_block: Option<u64>,
    ) -> Result<RpcEnum, NeonError> {
        Ok(if let Some(slot) = slot {
            RpcEnum::CallDbClient(
                CallDbClient::new(self.tracer_db.clone(), slot, tx_index_in_block).await?,
            )
        } else {
            RpcEnum::CloneRpcClient(self.rpc_client.clone())
        })
    }
}
