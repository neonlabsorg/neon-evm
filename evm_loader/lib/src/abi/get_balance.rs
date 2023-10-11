use super::params_to_neon_error;
use crate::commands::get_balance::{self, GetBalanceResponse};
use crate::commands::get_config::BuildConfigSimulator;
use crate::rpc::Rpc;
use crate::Config;
use crate::{types::GetBalanceRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    params: &str,
) -> NeonResult<Vec<GetBalanceResponse>> {
    let params: GetBalanceRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_balance::execute(rpc, &config.evm_loader, &params.account).await
}
