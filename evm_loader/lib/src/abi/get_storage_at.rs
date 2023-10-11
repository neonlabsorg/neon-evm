use super::params_to_neon_error;
use crate::commands::get_storage_at::{self, GetStorageAtReturn};
use crate::{types::request_models::GetStorageAtRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<GetStorageAtReturn> {
    let params: GetStorageAtRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_storage_at::execute(
        context.rpc_client,
        &config.evm_loader,
        params.contract_id,
        &params.index,
    )
    .await
}
