use async_trait::async_trait;
use jsonrpsee_core::{client::ClientT, rpc_params};
use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use neon_lib::LibMethods;
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
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{config::NeonRpcClientConfig, NeonRpcClient, NeonRpcClientResult};

pub struct NeonRpcHttpClient {
    client: HttpClient,
}

impl NeonRpcHttpClient {
    pub async fn new(config: NeonRpcClientConfig) -> NeonRpcClientResult<NeonRpcHttpClient> {
        Ok(NeonRpcHttpClient {
            client: HttpClientBuilder::default().build(config.url)?,
        })
    }
}

pub struct NeonRpcHttpClientBuilder {}

impl NeonRpcHttpClientBuilder {
    pub fn new() -> NeonRpcHttpClientBuilder {
        NeonRpcHttpClientBuilder {}
    }

    pub async fn build(&self, url: impl Into<String>) -> NeonRpcClientResult<NeonRpcHttpClient> {
        let config = NeonRpcClientConfig::new(url);
        NeonRpcHttpClient::new(config).await
    }
}

impl Default for NeonRpcHttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl NeonRpcClient for NeonRpcHttpClient {
    async fn cancel_trx(&self, params: CancelTrxRequest) -> NeonRpcClientResult<CancelTrxReturn> {
        self.request(LibMethods::CancelTrx, params).await
    }

    async fn collect_treasury(&self) -> NeonRpcClientResult<CollectTreasuryReturn> {
        self.request_without_params(LibMethods::CollectTreasury)
            .await
    }

    async fn create_ether_account(
        &self,
        params: CreateEtherAccountRequest,
    ) -> NeonRpcClientResult<CreateEtherAccountReturn> {
        self.request(LibMethods::CreateEtherAccount, params).await
    }

    async fn deposit(&self, params: DepositRequest) -> NeonRpcClientResult<DepositReturn> {
        self.request(LibMethods::Deposit, params).await
    }

    async fn emulate(
        &self,
        params: EmulateRequestModel,
    ) -> NeonRpcClientResult<EmulationResultWithAccounts> {
        self.request(LibMethods::Emulate, params).await
    }

    async fn get_ether_account_data(
        &self,
        params: GetEtherRequest,
    ) -> NeonRpcClientResult<GetEtherAccountDataReturn> {
        self.request(LibMethods::GetEtherAccountData, params).await
    }

    async fn get_neon_elf(
        &self,
        params: GetNeonElfRequest,
    ) -> NeonRpcClientResult<GetNeonElfReturn> {
        self.request(LibMethods::GetNeonElf, params).await
    }

    async fn get_storage_at(
        &self,
        params: GetStorageAtRequest,
    ) -> NeonRpcClientResult<GetStorageAtReturn> {
        self.request(LibMethods::GetStorageAt, params).await
    }

    async fn init_environment(
        &self,
        params: InitEnvironmentRequest,
    ) -> NeonRpcClientResult<InitEnvironmentReturn> {
        self.request(LibMethods::InitEnvironment, params).await
    }

    async fn trace(&self, params: TraceRequestModel) -> NeonRpcClientResult<serde_json::Value> {
        self.request(LibMethods::Trace, params).await
    }
}

impl NeonRpcHttpClient {
    async fn request<R, P>(&self, method: LibMethods, params: P) -> NeonRpcClientResult<R>
    where
        P: Serialize, // + jsonrpsee_core::traits::ToRpcParams + Send,
        R: DeserializeOwned,
    {
        Ok(self
            .client
            .request(method.into(), rpc_params![params])
            .await?)
    }

    async fn request_without_params<R>(&self, method: LibMethods) -> NeonRpcClientResult<R>
    where
        R: DeserializeOwned,
    {
        Ok(self.client.request(method.into(), rpc_params![]).await?)
    }
}

#[cfg(test)]
mod tests {
    use neon_lib::types::Address;

    use super::*;

    #[tokio::test]
    async fn test_get_ether_account_data() {
        let client = NeonRpcHttpClientBuilder::new()
            .build("http://localhost:3100/")
            .await
            .unwrap();

        let res = client
            .get_ether_account_data(GetEtherRequest {
                ether: Address::from_hex("0xFA8F24549bcC2448B024D794cAFB7807dC25E633").unwrap(),
                slot: None,
            })
            .await
            .unwrap();
        println!("{:?}", res);
    }
}
