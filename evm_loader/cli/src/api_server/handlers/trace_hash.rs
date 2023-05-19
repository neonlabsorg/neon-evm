use crate::{api_server::request_models::TxParamsRequest, context, NeonApiState};
use axum::{http::StatusCode, Json};

use crate::{api_server::state::State, context, types::request_models::TraceHashRequestModel};

use super::{parse_emulation_params, process_result};
use super::{parse_tx, parse_tx_params, process_error, process_result};
use crate::commands::emulate as EmulateCommand;

#[allow(clippy::unused_async)]
pub async fn trace_hash(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(trace_hash_request): Json<TraceHashRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tx: crate::types::TxParams = parse_tx(&tx_params_request);

    let signer = match context::build_singer(&state.config) {
        Ok(singer) => singer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client = match context::build_hash_rpc_client(
        &state.config,
        &trace_hash_request.emulate_hash_request.hash,
    ) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let tx = rpc_client.get_transaction_data()?;

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_hash_request.emulate_hash_request.emulation_params,
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
