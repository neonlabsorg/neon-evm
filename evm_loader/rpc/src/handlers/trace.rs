use super::invoke;
use crate::{context::Context, error::NeonRPCError};
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::TraceRequestModel, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params(params): Params<Vec<TraceRequestModel>>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let param = params.first().ok_or(NeonRPCError::IncorrectParameters())?;
    invoke(
        LibMethods::Trace,
        ctx,
        serde_json::value::to_value(param).unwrap(),
    )
    .await
}
