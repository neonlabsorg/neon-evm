#![allow(clippy::future_not_send)]

use actix_request_identifier::RequestId;
use actix_web::{http::StatusCode, post, web::Json, Responder};
use std::convert::Into;
use tracing::info;

use crate::api_server::handlers::process_error;
use crate::{
    commands::simulate_solana as SimulateSolanaCommand, types::SimulateSolanaRequest, NeonApiState,
};

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/simulate_solana")]
pub async fn simulate_solana(
    state: NeonApiState,
    request_id: RequestId,
    Json(emulate_request): Json<SimulateSolanaRequest>,
) -> impl Responder {
    info!("simulate_solana_request={:?}", emulate_request);

    let rpc = match state.build_rpc(None, None).await {
        Ok(rpc) => rpc,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &SimulateSolanaCommand::execute(&rpc, emulate_request)
            .await
            .map_err(Into::into),
    )
}
