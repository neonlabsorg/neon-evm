use super::params_to_neon_error;
use crate::commands::cancel_trx::{self, CancelTrxReturn};
use crate::{types::request_models::CancelTrxRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<CancelTrxReturn> {
    let signer = context.signer()?;

    let params: CancelTrxRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    cancel_trx::execute(
        context.rpc_client,
        signer.as_ref(),
        config.evm_loader,
        &params.storage_account,
    )
    .await
}
