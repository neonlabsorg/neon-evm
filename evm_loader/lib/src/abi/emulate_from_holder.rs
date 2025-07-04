use super::params_to_neon_error;
use crate::commands::emulate::{self, EmulateResponse};
use crate::commands::get_config::BuildConfigSimulator;
use crate::config::APIOptions;
use crate::{types::EmulateFromHolderApiRequest, NeonResult};

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    config: &APIOptions,
    params: &str,
) -> NeonResult<EmulateResponse> {
    let params: EmulateFromHolderApiRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    emulate::execute_from_holder(rpc, &config.evm_loader, params)
        .await
        .map(|(response, _)| response)
}
