use crate::commands::get_ether_account_data as GetEtherAccountDataCommand;
use crate::{api_context, context, types::request_models::GetEtherRequest, NeonApiState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::convert::Into;

use super::process_result;

#[tracing::instrument(skip(state))]
pub async fn get_ether_account_data(
    Query(req_params): Query<GetEtherRequest>,
    State(state): State<NeonApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rpc_client = api_context::build_rpc_client(&state, req_params.slot);

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
