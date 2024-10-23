#![allow(clippy::future_not_send)]

use async_trait::async_trait;
use jsonrpsee_core::{client::ClientT, rpc_params};
use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use neon_lib::types::GetBalanceWithPubkeyRequest;
use neon_lib::LibMethod;
use neon_lib::{
    commands::{
        emulate::EmulateResponse, get_balance::GetBalanceResponse, get_config::GetConfigResponse,
        get_contract::GetContractResponse, get_holder::GetHolderResponse,
        get_storage_at::GetStorageAtReturn, get_transaction_tree::GetTreeResponse,
    },
    types::{
        EmulateApiRequest, GetBalanceRequest, GetContractRequest, GetHolderRequest,
        GetStorageAtRequest, GetTransactionTreeRequest,
    },
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{config::NeonRpcClientConfig, NeonRpcClient, NeonRpcClientResult};

pub struct NeonRpcHttpClient {
    client: HttpClient,
}

impl NeonRpcHttpClient {
    pub fn new(config: NeonRpcClientConfig) -> NeonRpcClientResult<Self> {
        Ok(Self {
            client: HttpClientBuilder::default().build(config.url)?,
        })
    }
}

pub struct NeonRpcHttpClientBuilder {}

impl NeonRpcHttpClientBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    pub fn build(&self, url: impl Into<String>) -> NeonRpcClientResult<NeonRpcHttpClient> {
        let config = NeonRpcClientConfig::new(url);
        NeonRpcHttpClient::new(config)
    }
}

impl Default for NeonRpcHttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl NeonRpcClient for NeonRpcHttpClient {
    async fn emulate(&self, params: EmulateApiRequest) -> NeonRpcClientResult<EmulateResponse> {
        self.request(LibMethod::Emulate, params).await
    }

    async fn balance(
        &self,
        params: GetBalanceRequest,
    ) -> NeonRpcClientResult<Vec<GetBalanceResponse>> {
        self.request(LibMethod::GetBalance, params).await
    }

    async fn balance_with_pubkey(
        &self,
        params: GetBalanceWithPubkeyRequest,
    ) -> NeonRpcClientResult<Vec<GetBalanceResponse>> {
        self.request(LibMethod::GetBalanceWithPubkey, params).await
    }

    async fn get_contract(
        &self,
        params: GetContractRequest,
    ) -> NeonRpcClientResult<Vec<GetContractResponse>> {
        self.request(LibMethod::GetContract, params).await
    }

    async fn get_config(&self) -> NeonRpcClientResult<GetConfigResponse> {
        self.request_without_params(LibMethod::GetConfig).await
    }

    async fn get_holder(&self, params: GetHolderRequest) -> NeonRpcClientResult<GetHolderResponse> {
        self.request(LibMethod::GetHolder, params).await
    }

    async fn get_storage_at(
        &self,
        params: GetStorageAtRequest,
    ) -> NeonRpcClientResult<GetStorageAtReturn> {
        self.request(LibMethod::GetStorageAt, params).await
    }

    async fn get_transaction_tree(
        &self,
        params: GetTransactionTreeRequest,
    ) -> NeonRpcClientResult<GetTreeResponse> {
        self.request(LibMethod::GetTransactionTree, params).await
    }

    async fn trace(&self, params: EmulateApiRequest) -> NeonRpcClientResult<serde_json::Value> {
        self.request(LibMethod::Trace, params).await
    }
}

impl NeonRpcHttpClient {
    async fn request<R, P>(&self, method: LibMethod, params: P) -> NeonRpcClientResult<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        Ok(self
            .client
            .request(method.into(), rpc_params![params])
            .await?)
    }

    async fn request_without_params<R>(&self, method: LibMethod) -> NeonRpcClientResult<R>
    where
        R: DeserializeOwned,
    {
        Ok(self.client.request(method.into(), rpc_params![]).await?)
    }
}
