use crate::{api_context, context, types::request_models::GetStorageAtRequest, NeonApiState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::convert::Into;

use crate::commands::get_storage_at as GetStorageAtCommand;

use super::process_result;

#[tracing::instrument(skip(state))]
pub async fn get_storage_at(
    Query(req_params): Query<GetStorageAtRequest>,
    State(state): State<NeonApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rpc_client = api_context::build_rpc_client(&state, req_params.slot);

    let context = context::create(rpc_client, state.config.clone());

    process_result(
        &GetStorageAtCommand::execute(
            context.rpc_client.as_ref(),
            &state.config.evm_loader,
            req_params.contract_id,
            &req_params.index,
        )
        .await
        .map_err(Into::into),
    )
}
