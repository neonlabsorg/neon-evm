#![allow(clippy::future_not_send)]

use actix_request_identifier::RequestId;
use actix_web::{http::StatusCode, post, web::Json, Responder};
use std::convert::Into;
use tracing::info;

use crate::api_server::handlers::process_error;
use crate::{
    commands::emulate as EmulateCommand, types::EmulateFromHolderApiRequest, NeonApiState,
};

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/emulate_from_holder")]
pub async fn emulate_from_holder(
    state: NeonApiState,
    request_id: RequestId,
    Json(emulate_request): Json<EmulateFromHolderApiRequest>,
) -> impl Responder {
    info!("emulate_request={:?}", emulate_request);

    let slot = emulate_request.slot;
    let index = emulate_request.tx_index_in_block;

    let rpc = match state.build_rpc(slot, index).await {
        Ok(rpc) => rpc,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &EmulateCommand::execute_from_holder(&rpc, &state.config.evm_loader, emulate_request)
            .await
            .map(|(response, _)| response)
            .map_err(Into::into),
    )
}
