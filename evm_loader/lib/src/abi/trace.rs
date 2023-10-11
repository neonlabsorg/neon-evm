use serde_json::Value;

use super::{params_to_neon_error, parse_emulation_params};
use crate::commands::trace::{self};
use crate::{types::request_models::TraceRequestModel, NeonResult};
use crate::{Config, Context};

pub async fn execute(context: &Context<'_>, config: &Config, params: &str) -> NeonResult<Value> {
    let params: TraceRequestModel =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    let (token, chain, steps, accounts, solana_accounts) =
        parse_emulation_params(config, context, &params.emulate_request.emulation_params).await;

    trace::trace_transaction(
        context.rpc_client,
        config.evm_loader,
        params.emulate_request.tx_params.into(),
        token,
        chain,
        steps,
        config.commitment,
        &accounts,
        &solana_accounts,
        params.trace_call_config.unwrap_or_default(),
    )
    .await
}
