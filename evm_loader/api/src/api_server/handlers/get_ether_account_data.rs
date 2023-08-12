use crate::commands::get_ether_account_data as GetEtherAccountDataCommand;
use crate::{context, types::request_models::GetEtherRequest, NeonApiState};
use actix_web::{get, http::StatusCode, web, Responder};
use std::convert::Into;

use super::{process_error, process_result};

#[get("/get-ether-account-data")]
pub async fn get_ether_account_data(
    web::Query(req_params): web::Query<GetEtherRequest>,
    state: NeonApiState,
) -> impl Responder {
    let rpc_client = match context::build_rpc_client(&state.config, req_params.slot) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let context = context::create(rpc_client, state.config.clone());

    process_result(
        &GetEtherAccountDataCommand::execute(
            context.rpc_client.as_ref(),
            &state.config.evm_loader,
            &req_params.ether,
        )
        .await
        .map_err(Into::into),
    )
}
