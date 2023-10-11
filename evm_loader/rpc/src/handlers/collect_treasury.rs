use super::invoke;
use crate::context::Context;
use jsonrpc_v2::Data;
use neon_lib::LibMethods;

pub async fn handle(ctx: Data<Context>) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    invoke(
        LibMethods::CollectTreasury,
        ctx,
        serde_json::value::to_value("null").unwrap(),
    )
    .await
}
