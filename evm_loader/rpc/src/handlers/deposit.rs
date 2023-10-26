use super::invoke;
use crate::context::Context;
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::DepositRequest, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params((param,)): Params<(DepositRequest,)>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethods::Deposit, ctx, param).await
}
