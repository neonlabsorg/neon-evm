use super::invoke;
use crate::context::Context;
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::TraceRequestModel, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params((param,)): Params<(TraceRequestModel,)>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethods::Trace, ctx, param).await
}
