use axum::{http::StatusCode, Json};

use crate::{
    api_server::state::State, commands::emulate as EmulateCommand, context,
    types::request_models::EmulateHashRequestModel,
};

use super::{parse_emulation_params, process_error, process_result};

#[allow(clippy::unused_async)]
pub async fn emulate_hash(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(emulate_hash_request): Json<EmulateHashRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    let signer = match context::build_singer(&state.config) {
        Ok(singer) => singer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client = match context::build_hash_rpc_client(
        &state.config,
        emulate_hash_request.hash.as_deref().unwrap_or_default(),
    ) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let tx = rpc_client.get_transaction_data()?;

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &emulate_hash_request.emulation_params,
    );

    process_result(&EmulateCommand::execute(
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
