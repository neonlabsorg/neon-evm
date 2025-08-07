#![allow(clippy::future_not_send)]

use crate::api_server::handlers::process_error;
use crate::commands::get_container as GetContainerCommand;
use crate::{types::GetContainerRequest, NeonApiState};
use actix_request_identifier::RequestId;
use actix_web::post;
use actix_web::web::Json;
use actix_web::{http::StatusCode, Responder};
use std::convert::Into;
use tracing::info;

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/container")]
pub async fn get_container(
    state: NeonApiState,
    request_id: RequestId,
    Json(get_container_request): Json<GetContainerRequest>,
) -> impl Responder {
    info!("get_container_request={:?}", get_container_request);

    let rpc = match state.build_rpc(get_container_request.slot, None).await {
        Ok(rpc) => rpc,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &GetContainerCommand::execute(&rpc, state.config.evm_loader, get_container_request.pubkey)
            .await
            .map_err(Into::into),
    )
}
