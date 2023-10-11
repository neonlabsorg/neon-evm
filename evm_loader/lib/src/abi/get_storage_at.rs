use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_storage_at::{self, GetStorageAtReturn};
use crate::rpc::Rpc;
use crate::Config;
use crate::{types::GetStorageAtRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    params: &str,
) -> NeonResult<GetStorageAtReturn> {
    let params: GetStorageAtRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_storage_at::execute(rpc, &config.evm_loader, params.contract, params.index).await
}
