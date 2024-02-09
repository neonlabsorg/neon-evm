mod db_call_client;
mod validator_client;

pub use db_call_client::CallDbClient;
use std::time::Instant;
pub use validator_client::CloneRpcClient;

use crate::commands::get_config::{BuildConfigSimulator, ConfigSimulator};
use crate::{NeonError, NeonResult};
use async_trait::async_trait;
use clickhouse::Client;
use enum_dispatch::enum_dispatch;
use log::{error, info};
use solana_cli::cli::CliError;
use solana_client::{client_error::Result as ClientResult, rpc_response::RpcResult};
use solana_sdk::message::Message;
use solana_sdk::native_token::lamports_to_sol;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};

#[async_trait(?Send)]
#[enum_dispatch]
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
}

#[enum_dispatch(BuildConfigSimulator, Rpc)]
pub enum RpcEnum {
    CloneRpcClient,
    CallDbClient,
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

use crate::types::tracer_ch_common::{AccountRow, ChError};
pub(crate) use e;

pub(crate) async fn check_account_for_fee(
    rpc_client: &CloneRpcClient,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChDbConfig, TracerDb};
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_program_test::ProgramTest;
    use solana_sdk::account::AccountSharedData;
    use solana_sdk::signature::Signature;
    use solana_transaction_status::option_serializer::OptionSerializer;
    use solana_transaction_status::{UiLoadedAddresses, UiTransactionEncoding};
    use std::str::FromStr;

    #[tokio::test]
    async fn test() {
        let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        // let rpc_client = RpcClient::new(
        //     "https://neon-solrpc-mb-lb-1.solana.p2p.org/RkyUZGCodi5ZXrtlkZwzuMxUA7E80PQYav74ZkqWH"
        //         .to_string(),
        // );

        let slot = rpc_client.get_slot().await.unwrap();

        println!("slot: {slot}");

        let tx = rpc_client
            .get_transaction_with_config(&Signature::from_str("4yPDBk4YGvDs6cAMf315uoqTGdpjg7zAa5eiuY2YLhGSi863JTXHeRqzWZY5wMDnRvghDZ3zfsESFWCJFERzoufY").unwrap(), RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base58),
                commitment: None,
                max_supported_transaction_version: Some(0),
            })
            .await
            .unwrap();

        println!("tx {tx:?}");

        let loaded_addresses = match tx.transaction.meta.unwrap().loaded_addresses {
            OptionSerializer::Some(loaded_addreses) => loaded_addreses,
            OptionSerializer::None => unreachable!(),
            OptionSerializer::Skip => unreachable!(),
        };

        let tracer_db = TracerDb::new(&ChDbConfig {
            clickhouse_url: vec!["http://localhost:8123".to_string()],
            clickhouse_user: None,
            clickhouse_password: None,
        });

        let mut context = ProgramTest::default().start_with_context().await;

        for address in loaded_addresses.writable {
            let pubkey = Pubkey::from_str(&address).unwrap();

            // let account = rpc_client.get_account(&pubkey).await;
            let account = get_account(&tracer_db.client, pubkey, slot).await;

            println!("writable {pubkey} {account:?}");
            // context.set_account(&pubkey, &account.into());
        }

        for address in loaded_addresses.readonly {
            let pubkey = Pubkey::from_str(&address).unwrap();

            // let account = rpc_client.get_account(&pubkey).await;
            let account = get_account(&tracer_db.client, pubkey, slot).await;

            println!("readonly {pubkey} {account:?}");
            // context.set_account(&pubkey, &account.into());
        }

        let tx = tx.transaction.transaction.decode().unwrap();

        println!("decoded {tx:?}");

        for key in tx.message.static_account_keys() {
            let account = rpc_client.get_account(key).await.unwrap();
            println!("account {account:?}");
        }

        // for lookup in tx.message.address_table_lookups().unwrap() {
        //     let account = rpc_client.get_account(&lookup.account_key).await.unwrap();
        //     println!("lookup {account:?}");
        // }
    }

    #[test]
    fn test2() {
        let pubkey = Pubkey::from_str("51r4RKRKNA6eLoLc3eKuNzmQQoswp98B1TQiC4KRWHcP").unwrap();
        let pubkey_str = format!("{:?}", pubkey.to_bytes());

        println!("pubkey_str {pubkey_str}");
    }
}

async fn get_account(
    client: &Client,
    pubkey: Pubkey,
    slot: Slot,
) -> clickhouse::error::Result<AccountRow> {
    let pubkey_str = format!("{:?}", pubkey.to_bytes());

    client
        .query(
            r#"
                    SELECT owner, lamports, executable, rent_epoch, data, txn_signature
                    FROM events.update_account_distributed
                    WHERE pubkey = ?
                      AND slot < ?
                    ORDER BY slot DESC, write_version DESC
                    LIMIT 1
                "#,
        )
        .bind(pubkey_str)
        .bind(slot)
        .fetch_one::<AccountRow>()
        .await
}
