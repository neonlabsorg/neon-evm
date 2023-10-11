use super::{params_to_neon_error, parse_emulation_params};
use crate::commands::emulate::{self, EmulationResultWithAccounts};
use crate::{types::request_models::EmulateRequestModel, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<EmulationResultWithAccounts> {
    let params: EmulateRequestModel =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    let (token, chain, steps, accounts, solana_accounts) =
        parse_emulation_params(config, context, &params.emulation_params).await;

    emulate::execute(
        context.rpc_client,
        config.evm_loader,
        params.tx_params.into(),
        token,
        chain,
        steps,
        config.commitment,
        &accounts,
        &solana_accounts,
        &None,
        None,
    )
    .await
}
