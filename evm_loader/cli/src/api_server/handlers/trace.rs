use axum::{http::StatusCode, Json};

use crate::{api_server::request_models::TxParamsRequest, context, NeonApiState};

use super::{parse_emulation_params, process_error, process_result};

#[allow(clippy::unused_async)]
pub async fn trace(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(tx_params_request): Json<TraceRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tx: crate::types::TxParams = parse_tx(&tx_params_request);

    let signer = match context::build_singer(&state.config) {
        Ok(singer) => singer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client = match context::build_rpc_client(&state.config, tx_params_request.slot) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_request.emulate_request.emulation_params,
    );

    process_result(&crate::commands::trace::execute(
        &state.config,
        &context,
        tx,
        token,
        chain,
        steps,
        &accounts,
        &solana_accounts,
    ))
}
