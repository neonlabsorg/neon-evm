use crate::api_server::handlers::process_error;
use crate::{api_context, types::GetStorageAtRequest, NeonApiState};
use actix_request_identifier::RequestId;
use actix_web::post;
use actix_web::web::Json;
use actix_web::{http::StatusCode, Responder};
use std::convert::Into;
use tracing::info;

use crate::commands::get_storage_at as GetStorageAtCommand;

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/storage")]
pub async fn get_storage_at(
    state: NeonApiState,
    request_id: RequestId,
    Json(get_storage_at_request): Json<GetStorageAtRequest>,
) -> impl Responder {
    info!("get_storage_at_request={:?}", get_storage_at_request);

    let rpc_client =
        match api_context::build_rpc_client(&state, get_storage_at_request.slot, None).await {
            Ok(rpc_client) => rpc_client,
            Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
        };

    process_result(
        &GetStorageAtCommand::execute(
            rpc_client.as_ref(),
            &state.config.evm_loader,
            get_storage_at_request.contract,
            get_storage_at_request.index,
        )
        .await
        .map_err(Into::into),
    )
}
