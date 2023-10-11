use super::invoke;
use crate::{context::Context, error::NeonRPCError};
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::EmulateApiRequest, LibMethod};

pub async fn handle(
    ctx: Data<Context>,
    Params(params): Params<Vec<EmulateApiRequest>>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let param = params.first().ok_or(NeonRPCError::IncorrectParameters())?;
    invoke(
        LibMethod::Trace,
        ctx,
        Some(serde_json::value::to_value(param).unwrap()),
    )
    .await
}
