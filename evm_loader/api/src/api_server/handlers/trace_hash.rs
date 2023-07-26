use std::convert::Into;

use crate::{context, types::request_models::TraceHashRequestModel, NeonApiState};
use axum::{http::StatusCode, Json};
use neon_lib::account_storage::EmulatorAccountStorage;
use neon_lib::commands::emulate::setup_syscall_stubs;
use neon_lib::commands::trace::trace_transaction;
use neon_lib::types::trace::{TraceCallConfig, TracedCall};
use neon_lib::NeonError;

use super::{parse_emulation_params, process_result};

pub async fn trace_hash(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(trace_hash_request): Json<TraceHashRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    process_result(
        &trace_hash_helper(state, trace_hash_request)
            .await
            .map_err(Into::into),
    )
}

async fn trace_hash_helper(
    state: NeonApiState,
    trace_hash_request: TraceHashRequestModel,
) -> Result<TracedCall, NeonError> {
    let rpc_client = context::build_hash_rpc_client(
        &state.config,
        &trace_hash_request.emulate_hash_request.hash,
    )
    .await?; // TODO 400 -> 500 error

    let tx = rpc_client.get_transaction_data().await?; // TODO 400 -> 500 error

    let context = context::create(rpc_client, state.config.clone());

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_hash_request.emulate_hash_request.emulation_params,
    )
    .await;

    let rpc_client = context.rpc_client.as_ref();

    setup_syscall_stubs(&*rpc_client).await?;

    let trace_call_config: TraceCallConfig =
        trace_hash_request.trace_config.unwrap_or_default().into();

    let storage = EmulatorAccountStorage::with_accounts(
        &*rpc_client,
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

    trace_transaction(tx, chain, steps, &trace_call_config.trace_config, storage)
}
