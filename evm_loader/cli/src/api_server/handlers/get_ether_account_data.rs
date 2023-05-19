use crate::{api_server::request_models, context, NeonApiState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use evm_loader::types::Address;

use crate::commands::get_ether_account_data as GetEtherAccountDataCommand;

use super::{process_error, process_result};

#[allow(clippy::unused_async)]
pub async fn get_ether_account_data(
    Query(req_params): Query<GetEtherRequest>,
    State(state): State<NeonApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let address = match Address::from_hex(req_params.ether.clone().unwrap_or_default().as_str()) {
        Ok(address) => address,
        Err(_) => {
            return process_error(
                StatusCode::BAD_REQUEST,
                &crate::errors::NeonCliError::IncorrectAddress(
                    req_params.ether.unwrap_or_default(),
                ),
            )
        }
    };
    let signer = match context::build_singer(&state.config) {
        Ok(singer) => singer,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let rpc_client = match context::build_rpc_client(&state.config, req_params.slot) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let context = context::create(rpc_client, signer);

    process_result(&GetEtherAccountDataCommand::execute(
        &state.config,
        &context,
        &get_ether.ether,
    ))
}
