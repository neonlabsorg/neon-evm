use super::invoke;
use crate::context::Context;
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::GetEtherRequest, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params((param,)): Params<(GetEtherRequest,)>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethods::GetEtherAccountData, ctx, param).await
}
