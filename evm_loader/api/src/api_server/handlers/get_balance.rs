use crate::api_server::handlers::process_error;
use crate::commands::get_balance as GetBalanceCommand;
use crate::{api_context, types::GetBalanceRequest, NeonApiState};
use actix_request_identifier::RequestId;
use actix_web::web::Json;
use actix_web::{http::StatusCode, post, Responder};
use std::convert::Into;
use tracing::log::info;

use super::process_result;

#[tracing::instrument(skip_all, fields(id = request_id.as_str()))]
#[post("/balance")]
pub async fn get_balance(
    state: NeonApiState,
    request_id: RequestId,
    Json(get_balance_request): Json<GetBalanceRequest>,
) -> impl Responder {
    info!("get_balance_request={:?}", get_balance_request);

    let rpc_client =
        match api_context::build_rpc_client(&state, get_balance_request.slot, None).await {
            Ok(rpc_client) => rpc_client,
            Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
        };

    process_result(
        &GetBalanceCommand::execute(
            rpc_client.as_ref(),
            &state.config.evm_loader,
            &get_balance_request.account,
        )
        .await
        .map_err(Into::into),
    )
}
