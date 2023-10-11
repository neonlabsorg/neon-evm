use super::invoke;
use crate::{context::Context, error::NeonRPCError};
use jsonrpc_v2::{Data, Params};
use neon_lib::{types::request_models::GetEtherRequest, LibMethods};

pub async fn handle(
    ctx: Data<Context>,
    Params(params): Params<Vec<GetEtherRequest>>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let param = params.first().ok_or(NeonRPCError::IncorrectParameters())?;
    invoke(
        LibMethods::GetEtherAccountData,
        ctx,
        serde_json::value::to_value(param).unwrap(),
    )
    .await
}
