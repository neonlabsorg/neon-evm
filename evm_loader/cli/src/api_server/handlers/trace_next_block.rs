use axum::{http::StatusCode, Json};
use serde_json::json;

use crate::{
    api_server::handlers::process_error,
    commands::trace::trace_block,
    context, errors,
    types::{request_models::TraceNextBlockRequestModel, IndexerDb},
    NeonApiState,
};

use super::{parse_emulation_params, process_result};

#[allow(clippy::unused_async)]
pub async fn trace_next_block(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(trace_next_block_request): Json<TraceNextBlockRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    let signer = match context::build_signer(&state.config) {
        Ok(signer) => signer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client =
        match context::build_call_db_client(&state.config, trace_next_block_request.slot) {
            Ok(rpc_client) => rpc_client,
            Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
        };

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_next_block_request.emulation_params,
    );

    let indexer_db = IndexerDb::new(
        state
            .config
            .db_config
            .as_ref()
            .expect("db-config is required"),
    );

    let transactions = match indexer_db.get_block_transactions(trace_next_block_request.slot + 1) {
        Ok(transactions) => transactions,
        Err(e) => {
            return process_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &errors::NeonCliError::PostgreError(e),
            )
        }
    };

    process_result(
        &trace_block(
            context.rpc_client.as_ref(),
            state.config.evm_loader,
            transactions,
            token,
            chain,
            steps,
            state.config.commitment,
            &accounts,
            &solana_accounts,
            &trace_next_block_request.trace_config.unwrap_or_default(),
        )
        .map(|result| json!(result)),
    )
}
