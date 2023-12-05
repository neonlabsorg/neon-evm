use crate::api_server::handlers::process_error;
use crate::commands::get_holder as GetHolderCommand;
use crate::{api_context, types::GetHolderRequest, NeonApiState};
use actix_request_identifier::RequestId;
use actix_web::post;
use actix_web::web::Json;
use actix_web::{http::StatusCode, Responder};
use std::convert::Into;
use tracing::info;

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/holder")]
pub async fn get_holder_account_data(
    state: NeonApiState,
    request_id: RequestId,
    Json(get_holder_request): Json<GetHolderRequest>,
) -> impl Responder {
    info!("get_holder_request={:?}", get_holder_request);

    let rpc_client =
        match api_context::build_rpc_client(&state, get_holder_request.slot, None).await {
            Ok(rpc_client) => rpc_client,
            Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
        };

    process_result(
        &GetHolderCommand::execute(
            rpc_client.as_ref(),
            &state.config.evm_loader,
            get_holder_request.pubkey,
        )
        .await
        .map_err(Into::into),
    )
}
