#![allow(clippy::future_not_send)]

use crate::api_server::handlers::process_error;
use crate::commands::get_transaction_tree as GetTreeCommand;
use crate::types::GetTransactionTreeRequest;
use crate::NeonApiState;
use actix_request_identifier::RequestId;
use actix_web::post;
use actix_web::web::Json;
use actix_web::{http::StatusCode, Responder};
use std::convert::Into;
use tracing::info;

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/transaction_tree")]
pub async fn get_transaction_tree(
    state: NeonApiState,
    request_id: RequestId,
    Json(request): Json<GetTransactionTreeRequest>,
) -> impl Responder {
    info!("get_transaction_tree_request={:?}", request);

    let rpc = match state.build_rpc(request.slot, None).await {
        Ok(rpc) => rpc,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &GetTreeCommand::execute(
            &rpc,
            &state.config.evm_loader,
            request.origin,
            request.nonce,
        )
        .await
        .map_err(Into::into),
    )
}
