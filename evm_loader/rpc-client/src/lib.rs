#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc)]

mod config;
mod error;
pub mod http;

pub use error::NeonRpcClientError;

use async_trait::async_trait;
use neon_lib::{
    commands::{
        emulate::EmulateResponse, get_balance::GetBalanceResponse, get_config::GetConfigResponse,
        get_contract::GetContractResponse, get_holder::GetHolderResponse,
        get_storage_at::GetStorageAtReturn, get_transaction_tree::GetTreeResponse,
    },
    types::{
        EmulateApiRequest, GetBalanceRequest, GetBalanceWithPubkeyRequest, GetContractRequest,
        GetHolderRequest, GetStorageAtRequest, GetTransactionTreeRequest,
    },
};

type NeonRpcClientResult<T> = Result<T, NeonRpcClientError>;

#[async_trait(?Send)]
pub trait NeonRpcClient {
    async fn emulate(&self, params: EmulateApiRequest) -> NeonRpcClientResult<EmulateResponse>;
    async fn balance(
        &self,
        params: GetBalanceRequest,
    ) -> NeonRpcClientResult<Vec<GetBalanceResponse>>;
    async fn balance_with_pubkey(
        &self,
        params: GetBalanceWithPubkeyRequest,
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
    async fn get_transaction_tree(
        &self,
        params: GetTransactionTreeRequest,
    ) -> NeonRpcClientResult<GetTreeResponse>;
    async fn trace(&self, params: EmulateApiRequest) -> NeonRpcClientResult<serde_json::Value>;
}
