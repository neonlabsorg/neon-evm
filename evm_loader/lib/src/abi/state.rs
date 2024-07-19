use crate::config::APIOptions;
use crate::rpc::{CallDbClient, CloneRpcClient, RpcEnum};
use crate::types::{ClickHouseDb, RocksDb, TracerDbType};
use crate::NeonError;

pub struct State {
    pub tracer_db: TracerDbType,
    pub rpc_client: CloneRpcClient,
    pub config: APIOptions,
}

impl State {
    #[must_use]
    pub async fn new(config: APIOptions) -> Self {
        let tracer_db = match config.tracer_db_type.as_str() {
            "rocksdb" => RocksDb::new().await.into(),
            "clickhousedb" => ClickHouseDb::new().await.into(),
            _ => panic!("TRACER_DB_TYPE must be either 'ClickHouseDb' or 'RocksDb'"),
        };

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
