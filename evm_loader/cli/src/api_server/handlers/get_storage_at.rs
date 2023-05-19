use crate::{api_server::request_models::GetStorageAtRequest, context, NeonApiState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use evm_loader::types::Address;

use crate::commands::get_storage_at as GetStorageAtCommand;

use super::{process_error, process_result};

#[allow(clippy::unused_async)]
pub async fn get_storage_at(
    Query(req_params): Query<GetStorageAtRequest>,
    State(state): State<NeonApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let signer = context::build_singer(&state.config).map_err(|e| {
        tide::Error::from_str(
            400,
            format!("Error on creating singer: {:?}", e.to_string()),
        )
    })?;

    let rpc_client = match context::build_rpc_client(&state.config, req_params.slot) {
        Ok(rpc_client) => rpc_client,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    let context = context::create(rpc_client, signer);

    process_result(&GetStorageAtCommand::execute(
        &state.config,
        &context,
        req_params.contract_id,
        &req_params.index,
    ))
}
