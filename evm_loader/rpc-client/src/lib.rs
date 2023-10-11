mod config;
mod error;
pub mod http;

pub use error::NeonRpcClientError;

use async_trait::async_trait;
use neon_lib::{
    commands::{
        cancel_trx::CancelTrxReturn, collect_treasury::CollectTreasuryReturn,
        create_ether_account::CreateEtherAccountReturn, deposit::DepositReturn,
        emulate::EmulationResultWithAccounts, get_ether_account_data::GetEtherAccountDataReturn,
        get_neon_elf::GetNeonElfReturn, get_storage_at::GetStorageAtReturn,
        init_environment::InitEnvironmentReturn,
    },
    types::request_models::{
        CancelTrxRequest, CreateEtherAccountRequest, DepositRequest, EmulateRequestModel,
        GetEtherRequest, GetNeonElfRequest, GetStorageAtRequest, InitEnvironmentRequest,
        TraceRequestModel,
    },
};

type NeonRpcClientResult<T> = Result<T, NeonRpcClientError>;

#[async_trait(?Send)]
pub trait NeonRpcClient {
    async fn cancel_trx(&self, params: CancelTrxRequest) -> NeonRpcClientResult<CancelTrxReturn>;
    async fn collect_treasury(&self) -> NeonRpcClientResult<CollectTreasuryReturn>;
    async fn create_ether_account(
        &self,
        params: CreateEtherAccountRequest,
    ) -> NeonRpcClientResult<CreateEtherAccountReturn>;
    async fn deposit(&self, params: DepositRequest) -> NeonRpcClientResult<DepositReturn>;
    async fn emulate(
        &self,
        params: EmulateRequestModel,
    ) -> NeonRpcClientResult<EmulationResultWithAccounts>;
    async fn get_ether_account_data(
        &self,
        params: GetEtherRequest,
    ) -> NeonRpcClientResult<GetEtherAccountDataReturn>;
    async fn get_neon_elf(
        &self,
        params: GetNeonElfRequest,
    ) -> NeonRpcClientResult<GetNeonElfReturn>;
    async fn get_storage_at(
        &self,
        params: GetStorageAtRequest,
    ) -> NeonRpcClientResult<GetStorageAtReturn>;
    async fn init_environment(
        &self,
        params: InitEnvironmentRequest,
    ) -> NeonRpcClientResult<InitEnvironmentReturn>;
    async fn trace(&self, params: TraceRequestModel) -> NeonRpcClientResult<serde_json::Value>;
}
