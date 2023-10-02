use crate::errors::NeonError;
use crate::NeonApiResult;
use actix_web::http::StatusCode;
use actix_web::web::Json;
use serde::Serialize;
use serde_json::{json, Value};

use neon_lib::types::request_models::EmulationParamsRequestModel;
use neon_lib::types::EmulationParams;
use neon_lib::{types, RequestContext};
use std::net::AddrParseError;
use tracing::error;

pub mod build_info;
pub mod emulate;
pub mod get_ether_account_data;
pub mod get_storage_at;
pub mod trace;

#[derive(Debug)]
pub struct NeonApiError(pub NeonError);

impl NeonApiError {
    pub fn into_inner(self) -> NeonError {
        self.into()
    }
}

impl From<NeonError> for NeonApiError {
    fn from(value: NeonError) -> Self {
        NeonApiError(value)
    }
}

impl From<NeonApiError> for NeonError {
    fn from(value: NeonApiError) -> Self {
        value.0
    }
}

impl From<AddrParseError> for NeonApiError {
    fn from(value: AddrParseError) -> Self {
        NeonApiError(value.into())
    }
}

pub async fn parse_emulation_params(
    context: &RequestContext<'_>,
    params: EmulationParamsRequestModel,
) -> EmulationParams {
    let (token_mint, chain_id) =
        types::read_elf_params_if_none(context, params.token_mint.map(Into::into), params.chain_id)
            .await;

    EmulationParams {
        token_mint,
        chain_id,
        max_steps_to_execute: params.max_steps_to_execute,
        cached_accounts: params.cached_accounts.unwrap_or_default(),
        solana_accounts: params
            .solana_accounts
            .map(|vec| vec.into_iter().map(Into::into).collect())
            .unwrap_or_default(),
    }
}

fn process_result<T: Serialize>(
    result: &NeonApiResult<T>,
) -> (Json<serde_json::Value>, StatusCode) {
    match result {
        Ok(value) => (
            Json(json!({
                "result": "success",
                "value": value,
            })),
            StatusCode::OK,
        ),
        Err(e) => process_error(StatusCode::INTERNAL_SERVER_ERROR, &e.0),
    }
}

fn process_error(status_code: StatusCode, e: &NeonError) -> (Json<Value>, StatusCode) {
    error!("NeonError: {e}");
    (
        Json(json!({
            "result": "error",
            "error": e.to_string(),
        })),
        status_code,
    )
}
