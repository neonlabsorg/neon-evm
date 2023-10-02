use actix_request_identifier::RequestId;
use actix_web::{http::StatusCode, post, web::Json, Responder};
use std::convert::Into;

use crate::api_server::handlers::{parse_emulation_params, process_error};
use crate::commands::trace::trace_transaction;
use crate::{types::request_models::TraceRequestModel, NeonApiState};

use super::process_result;

#[tracing::instrument(skip(state, request_id), fields(id = request_id.as_str()))]
#[post("/trace")]
pub async fn trace(
    state: NeonApiState,
    request_id: RequestId,
    Json(trace_request): Json<TraceRequestModel>,
) -> impl Responder {
    let context = match state
        .request_context(
            trace_request.emulate_request.slot,
            trace_request.emulate_request.tx_index_in_block,
        )
        .await
    {
        Ok(context) => context,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &trace_transaction(
            &context,
            trace_request.emulate_request.tx_params,
            &parse_emulation_params(&context, trace_request.emulate_request.emulation_params).await,
            &trace_request.trace_call_config.unwrap_or_default(),
        )
        .await
        .map_err(Into::into),
    )
}
