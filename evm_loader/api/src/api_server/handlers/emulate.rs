use axum::{http::StatusCode, Json};
use std::convert::Into;

use crate::{
    api_context, commands::emulate as EmulateCommand, context,
    types::request_models::EmulateRequestModel, NeonApiState,
};

use super::{parse_emulation_params, process_result};

#[tracing::instrument(skip(state))]
pub async fn emulate(
    axum::extract::State(state): axum::extract::State<NeonApiState>,
    Json(emulate_request): Json<EmulateRequestModel>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tx = emulate_request.tx_params.into();

    let rpc_client = api_context::build_rpc_client(&state, emulate_request.slot);

    let context = context::create(rpc_client, state.config.clone());

    let (token, chain, steps, accounts, solana_accounts) =
        parse_emulation_params(&state.config, &context, &emulate_request.emulation_params).await;

    process_result(
        &EmulateCommand::execute(
            context.rpc_client.as_ref(),
            state.config.evm_loader,
            tx,
            token,
            chain,
            steps,
            state.config.commitment,
            &accounts,
            &solana_accounts,
            &None,
            None,
        )
        .await
        .map_err(Into::into),
    )
}
