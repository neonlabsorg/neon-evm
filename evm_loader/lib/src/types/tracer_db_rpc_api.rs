use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::account::Account;

// API
#[rpc(client, server)]
#[async_trait]
pub trait TracerDbApi {
    #[method(name = "get_block_time")]
    async fn get_block_time(&self, slot: u64) -> RpcResult<Option<i64>>;
    #[method(name = "get_earliest_rooted_slot")]
    async fn get_earliest_rooted_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_last_rooted_slot")]
    async fn get_last_rooted_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_slot_by_blockhash")]
    async fn get_slot_by_blockhash(&self, hash: &str) -> RpcResult<Option<u64>>;
    #[method(name = "get_transaction_index")]
    async fn get_transaction_index(&self, signature: &str) -> RpcResult<Option<u64>>;

    #[method(name = "get_account")]
    async fn get_account(
        &self,
        pubkey: &str,
        slot: u64,
        write_version: Option<u64>,
        bindata: Option<UiDataSliceConfig>,
    ) -> jsonrpsee::core::RpcResult<Option<Account>>;

    #[method(name = "get_accounts")]
    async fn get_accounts(&self, start: u64, end: u64) -> RpcResult<Vec<Vec<u8>>>;

    #[method(name = "get_accounts_in_transaction")]
    async fn get_accounts_in_transaction(
        &self,
        signature: &str,
        slot: Option<u64>,
    ) -> RpcResult<Vec<(String, Account)>>;
}
