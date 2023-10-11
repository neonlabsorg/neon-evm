use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_contract::{self, GetContractResponse};
use crate::rpc::Rpc;
use crate::Config;
use crate::{types::GetContractRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    params: &str,
) -> NeonResult<Vec<GetContractResponse>> {
    let params: GetContractRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_contract::execute(rpc, &config.evm_loader, &params.contract).await
}
