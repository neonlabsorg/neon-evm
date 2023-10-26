use super::invoke;
use crate::context::Context;
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::EmulateRequestModel, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params((param,)): Params<(EmulateRequestModel,)>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethods::Emulate, ctx, param).await
}
