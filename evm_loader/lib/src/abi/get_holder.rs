use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_holder::{self, GetHolderResponse};
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::{types::GetHolderRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &APIOptions,
    params: &str,
) -> NeonResult<GetHolderResponse> {
    let params: GetHolderRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_holder::execute(rpc, &config.evm_loader, params.pubkey).await
}
