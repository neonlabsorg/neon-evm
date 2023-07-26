use crate::{
    commands::trace::trace_block,
    context,
    types::{request_models::TraceNextBlockRequestModel, IndexerDb},
    NeonApiState,
};
use axum::http::StatusCode;
use axum::Json;
use neon_lib::account_storage::EmulatorAccountStorage;
use neon_lib::commands::emulate::setup_syscall_stubs;
use neon_lib::commands::trace::TraceBlockReturn;
use neon_lib::NeonError;
use std::sync::Arc;

use super::{parse_emulation_params, process_result};

pub async fn trace_next_block(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(trace_next_block_request): Json<TraceNextBlockRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    process_result(
        &trace_next_block_helper(state, trace_next_block_request)
            .await
            .map_err(Into::into),
    )
}

async fn trace_next_block_helper(
    state: NeonApiState,
    trace_next_block_request: TraceNextBlockRequestModel,
) -> Result<TraceBlockReturn, NeonError> {
    let rpc_client = context::build_call_db_client(&state.config, trace_next_block_request.slot)?; // TODO 400 -> 500 error

    let context = context::create(rpc_client, Arc::clone(&state.config));

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_next_block_request.emulation_params,
    )
    .await;

    let indexer_db = IndexerDb::new(
        state
            .config
            .db_config
            .as_ref()
            .expect("db-config is required"),
    )
    .await;

    // TODO: Query next block (which parent = slot) instead of getting slot + 1:
    let transactions = indexer_db
        .get_block_transactions(trace_next_block_request.slot + 1)
        .await?; // TODO Changed error type

    let rpc_client = context.rpc_client.as_ref();

    setup_syscall_stubs(rpc_client).await?;

    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        state.config.evm_loader,
        token,
        chain,
        state.config.commitment,
        &accounts,
        &solana_accounts,
        &None,
        None,
    )
    .await?;

    trace_block(
        transactions,
        chain,
        steps,
        &trace_next_block_request.trace_config.unwrap_or_default(),
        storage,
    )
    .await
}
