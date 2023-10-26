use super::invoke;
use crate::context::Context;
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::InitEnvironmentRequest, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params((param,)): Params<(InitEnvironmentRequest,)>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethods::InitEnvironment, ctx, param).await
}
