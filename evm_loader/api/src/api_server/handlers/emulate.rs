use actix_web::{http::StatusCode, post, web, Responder};
use std::convert::Into;

use crate::{
    commands::emulate as EmulateCommand, context, types::request_models::EmulateRequestModel,
    NeonApiState,
};

use super::{parse_emulation_params, process_error, process_result};

#[post("/emulate")]
pub async fn emulate(
    state: web::Data<NeonApiState>,
    web::Json(emulate_request): web::Json<EmulateRequestModel>,
) -> impl Responder {
    let tx = emulate_request.tx_params.into();

    let signer = match context::build_signer(&state.config) {
        Ok(signer) => signer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client = match context::build_rpc_client(&state.config, emulate_request.slot) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) =
        parse_emulation_params(&state.config, &context, &emulate_request.emulation_params).await;

    process_result(
        &EmulateCommand::execute(
            &state.config,
            &context,
            tx,
            token,
            chain,
            steps,
            &accounts,
            &solana_accounts,
        )
        .await
        .map_err(Into::into),
    )
}