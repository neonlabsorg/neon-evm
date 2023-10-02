use actix_request_identifier::RequestId;
use actix_web::{http::StatusCode, post, web::Json, Responder};
use std::convert::Into;

use crate::api_server::handlers::{parse_emulation_params, process_error};
use crate::{
    commands::emulate as EmulateCommand, types::request_models::EmulateRequestModel, NeonApiState,
};

use super::process_result;

#[tracing::instrument(skip(state, request_id), fields(id = request_id.as_str()))]
#[post("/emulate")]
pub async fn emulate(
    state: NeonApiState,
    request_id: RequestId,
    Json(emulate_request): Json<EmulateRequestModel>,
) -> impl Responder {
    let context = match state
        .request_context(emulate_request.slot, emulate_request.tx_index_in_block)
        .await
    {
        Ok(context) => context,
        Err(e) => return process_error(StatusCode::BAD_REQUEST, &e),
    };

    process_result(
        &EmulateCommand::execute(
            &context,
            emulate_request.tx_params,
            &parse_emulation_params(&context, emulate_request.emulation_params).await,
            None,
        )
        .await
        .map_err(Into::into),
    )
}
