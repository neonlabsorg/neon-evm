use axum::{http::StatusCode, Json};
use neon_lib::account_storage::EmulatorAccountStorage;
use neon_lib::commands::emulate::setup_syscall_stubs;
use neon_lib::types::trace::TracedCall;
use neon_lib::NeonError;
use std::convert::Into;

use crate::commands::trace::trace_transaction;
use crate::{context, types::request_models::TraceRequestModel, NeonApiState};

use super::{parse_emulation_params, process_result};

pub async fn trace(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(trace_request): Json<TraceRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    process_result(&trace_helper(state, trace_request).await.map_err(Into::into))
}

async fn trace_helper(
    state: NeonApiState,
    trace_request: TraceRequestModel,
) -> Result<TracedCall, NeonError> {
    let tx = trace_request.emulate_request.tx_params.into();

    let rpc_client = context::build_rpc_client(&state.config, trace_request.emulate_request.slot)?; // TODO 400 -> 500 error

    let context = context::create(rpc_client, state.config.clone());

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_request.emulate_request.emulation_params,
    )
    .await;

    let rpc_client = context.rpc_client.as_ref();

    setup_syscall_stubs(rpc_client).await?;

    let trace_call_config = trace_request.trace_call_config.unwrap_or_default();

    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        state.config.evm_loader,
        token,
        chain,
        state.config.commitment,
        &accounts,
        &solana_accounts,
        &trace_call_config.block_overrides,
        trace_call_config.state_overrides,
    )
    .await?;

    trace_transaction(tx, chain, steps, &trace_call_config.trace_config, storage).await
}
