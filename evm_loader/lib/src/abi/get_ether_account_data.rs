use super::params_to_neon_error;
use crate::commands::get_ether_account_data::{self, GetEtherAccountDataReturn};
use crate::{types::request_models::GetEtherRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<GetEtherAccountDataReturn> {
    let params: GetEtherRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_ether_account_data::execute(context.rpc_client, &config.evm_loader, &params.ether).await
}
