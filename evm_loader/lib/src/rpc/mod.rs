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
    use evm_loader::solana_program::instruction::Instruction;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_program_test::ProgramTest;
    use solana_sdk::account::AccountSharedData;
    use solana_sdk::instruction::AccountMeta;
    use solana_sdk::signature::Signature;
    use solana_sdk::transaction::Transaction;
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
            if let Ok(account) = account {
                let account: Account = account.try_into().unwrap();
                context.set_account(&pubkey, &account.into());
            }
        }

        for address in loaded_addresses.readonly {
            let pubkey = Pubkey::from_str(&address).unwrap();

            // let account = rpc_client.get_account(&pubkey).await;
            let account = get_account(&tracer_db.client, pubkey, slot).await;

            println!("readonly {pubkey} {account:?}");
            if let Ok(account) = account {
                let account: Account = account.try_into().unwrap();
                context.set_account(&pubkey, &account.into());
            }
        }

        let neon_evm_pubkey =
            Pubkey::from_str("NeonVMyRX5GbCrsAHnUwx1nYYoJAtskU1bWUo6JGNyG").unwrap();

        // let account = rpc_client.get_account(&pubkey).await;
        let account = get_account(&tracer_db.client, neon_evm_pubkey, slot).await;

        println!("NeonEVM {neon_evm_pubkey} {account:?}");
        if let Ok(account) = account {
            let account: Account = account.try_into().unwrap();
            context.set_account(&neon_evm_pubkey, &account.into());
        }

        let tx = tx.transaction.transaction.decode().unwrap();

        println!("decoded {tx:?}");

        let address_lookup_table_key = tx
            .message
            .address_table_lookups()
            .unwrap()
            .first()
            .unwrap()
            .account_key;

        // let account = rpc_client.get_account(&pubkey).await;
        let account = get_account(&tracer_db.client, address_lookup_table_key, slot).await;

        println!("address_lookup_table_account {address_lookup_table_key} {account:?}");
        if let Ok(account) = account {
            let account: Account = account.try_into().unwrap();
            context.set_account(&address_lookup_table_key, &account.into());
        }

        // for key in tx.message.static_account_keys() {
        //     let account = rpc_client.get_account(key).await.unwrap();
        //     println!("account {account:?}");
        // }

        // for lookup in tx.message.address_table_lookups().unwrap() {
        //     let account = rpc_client.get_account(&lookup.account_key).await.unwrap();
        //     println!("lookup {account:?}");
        // }

        // let tx = Transaction::new_unsigned(Message::new(tx.message.instructions()));

        let tx = Transaction::new_unsigned(Message::new(
            &[Instruction::new_with_bytes(
                neon_evm_pubkey,
                &tx.message.instructions().last().unwrap().data,
                vec![
                    AccountMeta {
                        pubkey: Pubkey::from_str("5ysUSZknqkoYYQKF6Dbt92d5aTWBt5H2p2y8sY5fLJpp")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("5GRKYKDwhsjARx1NUUUpyEun3ANGtAmRCYXLC4yyVWid")
                            .unwrap(),
                        is_signer: true,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("51r4RKRKNA6eLoLc3eKuNzmQQoswp98B1TQiC4KRWHcP")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("AG4GfVijKTfxwtxRXTSVdxXmMBHdyD7XmUxiHTcu7NpC")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("11111111111111111111111111111111").unwrap(),
                        is_signer: false,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("NeonVMyRX5GbCrsAHnUwx1nYYoJAtskU1bWUo6JGNyG")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("2hzhEy9GJUYM3v7uocc2TtuuHrvYwTHy4wXiyLnQTGf9")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("48W4UZeZuR1fc1strDG2qjctHkK6B6ueqYBXcsT9ge9W")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("4bwQcuoDPg2rsfjjSwBT2oARfFBm34M7aoWwnRmRje66")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("53L7YhhhA4J9KhAy1Fmfc2ReYQTDcrPj77Me6Vdp7YFx")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("5KuJMkRzVGgzy9EYQHMjsGBhZe2ZXouCeJ5gjn7d4WKj")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: Pubkey::from_str("5Lm7nBLnoKouQzg2ULVQkzJ57PfafRctzsUsJigHsPtU")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    }, // 12
                    AccountMeta {
                        pubkey: Pubkey::from_str("6EscUPSWFVHpbtoELx6CmR4TdNUq77yeHpbVx77ZUB9u")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 13
                    AccountMeta {
                        pubkey: Pubkey::from_str("7AL3iWyLCmKmxuaCXZDiDsYUMaB1iwrSDLmSryihQrAd")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 14
                    AccountMeta {
                        pubkey: Pubkey::from_str("9bBrVxJkX61vhuYXzJ8K91JtFrctDbesReju6qf5pkM1")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 15
                    AccountMeta {
                        pubkey: Pubkey::from_str("AXisyaUthrsf9nxai11AuVrJzyC7rrxuthVGFDo4MvUp")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 16
                    AccountMeta {
                        pubkey: Pubkey::from_str("AYkPFpqXyuD4pZvMJtRXBhD2vgTrGcKGs7hV5Ynr45ga")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    }, // 17
                    AccountMeta {
                        pubkey: Pubkey::from_str("BhPPTPCPSSDjBLqiZpwjVSmDNrTHBUBgGx746vuWhEHg")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 18
                    AccountMeta {
                        pubkey: Pubkey::from_str("Bvh7ZoJyhrrkVRVQTNENSQENae84gAH8WxdzUNVrBahP")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    }, // 19
                    AccountMeta {
                        pubkey: Pubkey::from_str("BypdCd5tuJnyoihHg3SVW7qqRsqyVVccg2EURUX3twrg")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 20
                    AccountMeta {
                        pubkey: Pubkey::from_str("C9ouTkDQWzgLfwJhvB9c3LiKTYTZFUiqvS6P9FdRh8LU")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 21
                    AccountMeta {
                        pubkey: Pubkey::from_str("CHQjfH7AaxHBfgi8g8HRcZGPJgNMjQ3ofNroEmZUsoXs")
                            .unwrap(),
                        is_signer: false,
                        is_writable: false,
                    }, // 22
                    AccountMeta {
                        pubkey: Pubkey::from_str("CWLGxiFYHKi6YDHgED2nEGP3DnSyKVGBgXt52SKMturu")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    }, // 23
                    AccountMeta {
                        pubkey: Pubkey::from_str("CWLGxiFYHKi6YDHgED2nEGP3DnSyKVGBgXt52SKMturu")
                            .unwrap(),
                        is_signer: false,
                        is_writable: true,
                    }, // 24
                ],
            )],
            None,
        ));

        let result = context.banks_client.simulate_transaction(tx).await.unwrap();

        println!("simulation result = {result:?}");
    }

    #[test]
    fn test2() {
        let pubkey = Pubkey::from_str("G9162vkmcM5hAswEeKCaAVcWwmWhnMxcuoszXmjsdyrS").unwrap();
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
