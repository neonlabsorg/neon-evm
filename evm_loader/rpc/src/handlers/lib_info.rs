use super::lib_build_info;
use crate::context::Context;
use jsonrpc_v2::Data;

pub async fn handle(ctx: Data<Context>) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    lib_build_info(ctx).await
}
