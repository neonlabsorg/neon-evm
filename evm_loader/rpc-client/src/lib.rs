#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc)]

mod config;
mod error;
pub mod http;

pub use error::NeonRpcClientError;

use async_trait::async_trait;
use neon_lib::{
    build_info_common::SlimBuildInfo,
    commands::{
        emulate::EmulateResponse, get_balance::GetBalanceResponse, get_config::GetConfigResponse,
        get_contract::GetContractResponse, get_holder::GetHolderResponse,
        get_storage_at::GetStorageAtReturn, simulate_solana::SimulateSolanaResponse,
    },
    types::{
        EmulateApiRequest, GetBalanceRequest, GetContractRequest, GetHolderRequest,
        GetStorageAtRequest, SimulateSolanaRequest,
    },
};

type NeonRpcClientResult<T> = Result<T, NeonRpcClientError>;

#[async_trait]
pub trait NeonRpcClient: Sync + Send + 'static {
    async fn emulate(&self, params: EmulateApiRequest) -> NeonRpcClientResult<EmulateResponse>;
    async fn balance(
        &self,
        params: GetBalanceRequest,
    ) -> NeonRpcClientResult<Vec<GetBalanceResponse>>;
    async fn get_contract(
        &self,
        params: GetContractRequest,
    ) -> NeonRpcClientResult<Vec<GetContractResponse>>;
    async fn get_holder(&self, params: GetHolderRequest) -> NeonRpcClientResult<GetHolderResponse>;
    async fn get_config(&self) -> NeonRpcClientResult<GetConfigResponse>;
    async fn get_storage_at(
        &self,
        params: GetStorageAtRequest,
    ) -> NeonRpcClientResult<GetStorageAtReturn>;
    async fn trace(&self, params: EmulateApiRequest) -> NeonRpcClientResult<serde_json::Value>;
    async fn simulate_solana(
        &self,
        params: SimulateSolanaRequest,
    ) -> NeonRpcClientResult<SimulateSolanaResponse>;
    async fn build_info(&self) -> NeonRpcClientResult<SlimBuildInfo>;
    async fn lib_build_info(&self) -> NeonRpcClientResult<SlimBuildInfo>;
}
