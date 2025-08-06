use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_container::{self, GetContainerResponse};
use crate::config::APIOptions;
use crate::types::GetContainerRequest;
use crate::NeonResult;

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    config: &APIOptions,
    params: &str,
) -> NeonResult<GetContainerResponse> {
    let params: GetContainerRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_container::execute(rpc, config.evm_loader, params.pubkey).await
}
