use actix_web::{http::StatusCode, post, web, Responder};
use std::convert::Into;

use crate::{
    commands::emulate as EmulateCommand,
    context,
    types::{request_models::EmulateHashRequestModel, trace::TraceCallConfig},
    NeonApiState,
};

use super::{parse_emulation_params, process_error, process_result};

#[post("/emulate_hash")]
pub async fn emulate_hash(
    state: web::Data<NeonApiState>,
    web::Json(emulate_hash_request): web::Json<EmulateHashRequestModel>,
) -> impl Responder {
    let signer = match context::build_signer(&state.config) {
        Ok(signer) => signer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let (rpc_client, blocking_rpc_client) =
        match context::build_hash_rpc_client(&state.config, &emulate_hash_request.hash).await {
            Ok(rpc_client) => rpc_client,
            Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
        };

    let tx = match rpc_client.get_transaction_data().await {
        Ok(tx) => tx,
        Err(e) => {
            return process_error(
                StatusCode::BAD_REQUEST,
                &crate::errors::NeonError::SolanaClientError(e),
            )
        }
    };

    let context = context::create(rpc_client, signer, blocking_rpc_client);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &emulate_hash_request.emulation_params,
    )
    .await;

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
            TraceCallConfig::default(),
        )
        .await
        .map_err(Into::into),
    )
}
