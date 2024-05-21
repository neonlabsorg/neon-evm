#![allow(clippy::future_not_send)]

use super::invoke;
use crate::context::Context;
use jsonrpc_v2::Data;
use neon_lib::LibMethod;

pub async fn handle(ctx: Data<Context>) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(LibMethod::GetConfig, ctx, Option::<serde_json::Value>::None).await
}
