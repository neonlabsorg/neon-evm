#![allow(clippy::future_not_send)]

use super::invoke;
use crate::{context::Context, error::NeonRPCError};
use jsonrpc_v2::{Data, Params};
use neon_lib::{
    types::{GetBalanceRequest, GetBalanceWithPubkeyRequest},
    LibMethod,
};

pub async fn handle(
    ctx: Data<Context>,
    Params(params): Params<Vec<GetBalanceRequest>>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let param = params.first().ok_or(NeonRPCError::IncorrectParameters())?;
    invoke(
        LibMethod::GetBalance,
        ctx,
        Some(serde_json::value::to_value(param).unwrap()),
    )
    .await
}

pub async fn handle_with_pubkey(
    ctx: Data<Context>,
    Params(params): Params<Vec<GetBalanceWithPubkeyRequest>>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let param = params.first().ok_or(NeonRPCError::IncorrectParameters())?;
    invoke(
        LibMethod::GetBalanceWithPubkey,
        ctx,
        Some(serde_json::value::to_value(param).unwrap()),
    )
    .await
}
