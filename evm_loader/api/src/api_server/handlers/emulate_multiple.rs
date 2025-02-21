#![allow(clippy::future_not_send)]

use actix_request_identifier::RequestId;
use actix_web::{http::StatusCode, post, web::Json, Responder};
use std::convert::Into;
use tracing::info;

use crate::api_server::handlers::process_error;
use crate::{
    commands::emulate_multiple as EmulateMultipleCommand, types::EmulateMultipleRequest,
    NeonApiState,
};

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/emulate_multiple")]
pub async fn emulate_multiple(
    state: NeonApiState,
    request_id: RequestId,
    Json(request): Json<EmulateMultipleRequest>,
) -> impl Responder {
    info!("emulate_multiple_request={:?}", request);

    let slot = request.slot;
    let index = request.tx_index_in_block;

    let rpc = match state.build_rpc(slot, index).await {
        Ok(rpc) => rpc,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &EmulateMultipleCommand::execute(
            &rpc,
            &state.config.db_config,
            &state.config.evm_loader,
            request,
        )
        .await
        .map_err(Into::into),
    )
}
