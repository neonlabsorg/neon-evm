#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic)]

#[allow(clippy::module_name_repetitions)]
mod build_info;
mod config;
mod logs;
mod program_options;

use neon_lib::{
    commands::{
        cancel_trx, collect_treasury, create_ether_account, deposit, emulate,
        get_ether_account_data, get_neon_elf, get_storage_at, init_environment, trace,
    },
    errors,
    types::{self, AccessListItem},
    RequestContext,
};

use clap::ArgMatches;
pub use config::Config;
use std::io::Read;

use ethnum::U256;
use evm_loader::evm::tracing::TraceCallConfig;
use log::debug;
use serde_json::json;
use solana_clap_utils::input_parsers::{pubkey_of, value_of, values_of};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::time::Instant;

pub use neon_lib::context::*;
use neon_lib::rpc::{CallDbClient, Rpc};

use crate::build_info::get_build_info;
use crate::{errors::NeonError, types::TransactionParams};
use evm_loader::types::Address;
use neon_lib::types::request_models::TxParamsRequestModel;
use neon_lib::types::{read_elf_params_if_none, EmulationParams, TracerDb};

type NeonCliResult = Result<serde_json::Value, NeonError>;

async fn run<'a>(options: &'a ArgMatches<'a>) -> NeonCliResult {
    let slot: Option<u64> = options
        .value_of("slot")
        .map(|slot_str| slot_str.parse().expect("slot parse error"));

    let config = config::create(options)?;

    let (cmd, params) = options.subcommand();

    let rpc_client = build_rpc_client(slot, &config).await?;

    let context = RequestContext::new(rpc_client, &config)?;

    execute(cmd, params, &context).await
}

async fn build_rpc_client(slot: Option<u64>, config: &Config) -> Result<Arc<dyn Rpc>, NeonError> {
    Ok(if let Some(slot) = slot {
        Arc::new(
            CallDbClient::new(
                TracerDb::new(config.db_config.as_ref().expect("db-config not found")),
                slot,
                None,
            )
            .await?,
        )
    } else {
        Arc::new(RpcClient::new_with_commitment(
            config.json_rpc_url.clone(),
            config.commitment,
        ))
    })
}

fn print_result(result: &NeonCliResult) {
    let logs = {
        let context = logs::CONTEXT.lock().unwrap();
        context.clone()
    };

    let result = match result {
        Ok(value) => serde_json::json!({
            "result": "success",
            "value": value,
            "logs": logs
        }),
        Err(e) => serde_json::json!({
            "result": "error",
            "error": e.to_string(),
            "logs": logs
        }),
    };

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let time_start = Instant::now();

    let options = program_options::parse();

    logs::init(&options).expect("logs init error");
    std::panic::set_hook(Box::new(|info| {
        let message = std::format!("Panic: {info}");
        print_result(&Err(NeonError::Panic(message)));
    }));

    debug!("{}", get_build_info());

    let result = run(&options).await;

    let execution_time = Instant::now().duration_since(time_start);
    log::info!("execution time: {} sec", execution_time.as_secs_f64());
    print_result(&result);
    if let Err(e) = result {
        std::process::exit(e.error_code());
    };
}

#[allow(clippy::too_many_lines)]
async fn execute<'a>(
    cmd: &str,
    params: Option<&'a ArgMatches<'a>>,
    context: &'a RequestContext<'_>,
) -> NeonCliResult {
    match (cmd, params) {
        ("emulate", Some(params)) => {
            let (tx_params, trace_call_config) = parse_tx_params(params);
            emulate::execute(
                context,
                tx_params,
                &parse_emulation_params(context, params).await,
                Some(&trace_call_config),
            )
            .await
            .map(|result| json!(result))
        }
        ("trace", Some(params)) => {
            let (tx_params, trace_call_config) = parse_tx_params(params);
            trace::trace_transaction(
                context,
                tx_params,
                &parse_emulation_params(context, params).await,
                &trace_call_config,
            )
            .await
            .map(|trace| json!(trace))
        }
        ("create-ether-account", Some(params)) => {
            let ether = address_of(params, "ether").expect("ether parse error");
            create_ether_account::execute(context, &ether)
                .await
                .map(|result| json!(result))
        }
        ("deposit", Some(params)) => {
            let amount = value_of(params, "amount").expect("amount parse error");
            let ether = address_of(params, "ether").expect("ether parse error");
            deposit::execute(context, amount, &ether)
                .await
                .map(|result| json!(result))
        }
        ("get-ether-account-data", Some(params)) => {
            let ether = address_of(params, "ether").expect("ether parse error");
            get_ether_account_data::execute(context, &ether)
                .await
                .map(|result| json!(result))
        }
        ("cancel-trx", Some(params)) => {
            let storage_account =
                pubkey_of(params, "storage_account").expect("storage_account parse error");
            cancel_trx::execute(context, &storage_account)
                .await
                .map(|result| json!(result))
        }
        ("neon-elf-params", Some(params)) => {
            let program_location = params.value_of("program_location");
            get_neon_elf::execute(context, program_location)
                .await
                .map(|result| json!(result))
        }
        ("collect-treasury", Some(_)) => collect_treasury::execute(context)
            .await
            .map(|result| json!(result)),
        ("init-environment", Some(params)) => {
            let file = params.value_of("file");
            let send_trx = params.is_present("send-trx");
            let force = params.is_present("force");
            let keys_dir = params.value_of("keys-dir");
            init_environment::execute(context, send_trx, force, keys_dir, file)
                .await
                .map(|result| json!(result))
        }
        ("get-storage-at", Some(params)) => {
            let contract_id = address_of(params, "contract_id").expect("contract_it parse error");
            let index = u256_of(params, "index").expect("index parse error");
            get_storage_at::execute(context, contract_id, &index)
                .await
                .map(|hash| json!(hex::encode(hash.0)))
        }
        _ => unreachable!(),
    }
}

fn parse_tx_params(params: &ArgMatches) -> (TxParamsRequestModel, TraceCallConfig) {
    let transaction_params: Option<TransactionParams> = read_from_stdin().unwrap_or_else(|err| {
        panic!("Unable to parse `TransactionParams` from STDIN, error: {err:?}")
    });
    let (data, trace_config) = transaction_params
        .map(|params| {
            (
                params.data.map(Into::into),
                params.trace_config.unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let tx_params = TxParamsRequestModel {
        nonce: None,
        sender: address_of(params, "sender").expect("sender parse error"),
        contract: address_or_deploy_of(params, "contract"),
        data,
        value: u256_of(params, "value"),
        gas_limit: u256_of(params, "gas_limit"),
        gas_price: u256_of(params, "gas_price"),
        access_list: access_list_of(params, "access_list"),
    };

    (tx_params, trace_config)
}

async fn parse_emulation_params<'a>(
    context: &RequestContext<'_>,
    params: &'a ArgMatches<'a>,
) -> EmulationParams {
    let (token_mint, chain_id) = read_elf_params_if_none(
        context,
        pubkey_of(params, "token_mint"),
        value_of(params, "chain_id"),
    )
    .await;

    EmulationParams {
        token_mint,
        chain_id,
        max_steps_to_execute: value_of::<u64>(params, "max_steps_to_execute")
            .expect("max_steps_to_execute parse error"),
        cached_accounts: values_of::<Address>(params, "cached_accounts").unwrap_or_default(),
        solana_accounts: values_of::<Pubkey>(params, "solana_accounts").unwrap_or_default(),
    }
}

fn address_or_deploy_of(matches: &ArgMatches<'_>, name: &str) -> Option<Address> {
    if matches.value_of(name) == Some("deploy") {
        return None;
    }
    address_of(matches, name)
}

fn address_of(matches: &ArgMatches<'_>, name: &str) -> Option<Address> {
    matches
        .value_of(name)
        .map(|value| Address::from_hex(value).unwrap())
}

fn access_list_of(matches: &ArgMatches<'_>, name: &str) -> Option<Vec<AccessListItem>> {
    matches.value_of(name).map(|value| {
        let address = Address::from_hex(value).unwrap();
        let keys = vec![];
        let item = AccessListItem {
            address,
            storage_keys: keys,
        };
        vec![item]
    })
}

fn u256_of(matches: &ArgMatches<'_>, name: &str) -> Option<U256> {
    matches.value_of(name).map(|value| {
        if value.is_empty() {
            return U256::ZERO;
        }

        U256::from_str_prefixed(value).unwrap()
    })
}

fn read_from_stdin<T: serde::de::DeserializeOwned>() -> serde_json::Result<Option<T>> {
    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(serde_json::Error::io)?;
    if stdin.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&stdin).map(Some)
}
