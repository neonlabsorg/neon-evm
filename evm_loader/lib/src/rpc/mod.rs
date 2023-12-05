mod db_call_client;
mod validator_client;

pub use db_call_client::CallDbClient;
pub use validator_client::SolanaRpc;

use crate::{NeonError, NeonResult};
use async_trait::async_trait;
use solana_cli::cli::CliError;
use solana_client::{
    client_error::Result as ClientResult, nonblocking::rpc_client::RpcClient,
    rpc_response::RpcResult,
};
use solana_sdk::message::Message;
use solana_sdk::native_token::lamports_to_sol;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};
use std::any::Any;

#[async_trait(?Send)]
pub trait Rpc {
    async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>>;
    async fn get_account_with_commitment(
        &self,
        key: &Pubkey,
        commitment: CommitmentConfig,
    ) -> RpcResult<Option<Account>>;
    async fn get_multiple_accounts(&self, pubkeys: &[Pubkey])
        -> ClientResult<Vec<Option<Account>>>;
    async fn get_block_time(&self, slot: Slot) -> ClientResult<UnixTimestamp>;
    async fn get_slot(&self) -> ClientResult<Slot>;

    fn as_any(&self) -> &dyn Any;
}

macro_rules! e {
    ($mes:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!("{}", $mes)))
    };
    ($mes:expr, $error:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!("{}: {:?}", $mes, $error)))
    };
    ($mes:expr, $error:expr, $arg:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!(
            "{}, {:?}: {:?}",
            $mes, $error, $arg
        )))
    };
}
pub(crate) use e;

pub(crate) async fn check_account_for_fee(
    rpc_client: &RpcClient,
    account_pubkey: &Pubkey,
    message: &Message,
) -> NeonResult<()> {
    let fee = rpc_client.get_fee_for_message(message).await?;
    let balance = rpc_client.get_balance(account_pubkey).await?;
    if balance != 0 && balance >= fee {
        return Ok(());
    }

    Err(NeonError::CliError(CliError::InsufficientFundsForFee(
        lamports_to_sol(fee),
        *account_pubkey,
    )))
}
