#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic)]

mod logs;
mod program_options;

use std::process::exit;
use std::str::FromStr;

use clap::ArgMatches;
use ethnum::U256;
use solana_clap_utils::input_parsers::{pubkey_of, value_of, values_of};
use solana_sdk::pubkey::Pubkey;

pub use config::Config;
use evm_loader::types::Address;
use neon_cli::{
    commands::{
        get_neon_elf::CachedElfParams, cancel_trx, collect_treasury, create_ether_account, deposit,
        emulate, get_ether_account_data, get_neon_elf, get_storage_at, init_environment, trace,
    },
    config,
    NeonCliResult,
    parsing::truncate_0x,
    types::TxParams,
};

#[tokio::main]
async fn main() {
    let options = program_options::parse();

    logs::init(&options).expect("logs init error");

    let config = config::create(&options);

    let (cmd, params) = options.subcommand();

    let result = execute(cmd, params, &config);
    let logs = {
        let context = crate::logs::CONTEXT.lock().unwrap();
        context.clone()
    };

    let (result, exit_code) = match result {
        Ok(result) => (
            serde_json::json!({
                "result": "success",
                "value": result,
                "logs": logs
            }),
            0_i32,
        ),
        Err(e) => {
            let error_code = e.error_code();
            (
                serde_json::json!({
                    "result": "error",
                    "error": e.to_string(),
                    "logs": logs
                }),
                error_code,
            )
        }
    };

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    exit(exit_code);
}

fn execute(cmd: &str, params: Option<&ArgMatches>, config: &Config) -> NeonCliResult {
    match (cmd, params) {
        ("emulate", Some(params)) => {
            let tx = parse_tx(params);
            let (token, chain, steps, accounts) = parse_tx_params(config, params);
            emulate::execute(config, tx, token, chain, steps, &accounts)
        }
        ("emulate_hash", Some(params)) => {
            let tx = config.rpc_client.get_transaction_data()?;
            let (token, chain, steps, accounts) = parse_tx_params(config, params);
            emulate::execute(config, tx, token, chain, steps, &accounts)
        }
        ("trace", Some(params)) => {
            let tx = parse_tx(params);
            let (token, chain, steps, accounts) = parse_tx_params(config, params);
            trace::execute(config, tx, token, chain, steps, &accounts, parse_enable_return_data(params))
        }
        ("trace_hash", Some(params)) => {
            let tx = config.rpc_client.get_transaction_data()?;
            let (token, chain, steps, accounts) = parse_tx_params(config, params);
            trace::execute(config, tx, token, chain, steps, &accounts, parse_enable_return_data(params))
        }
        ("create-ether-account", Some(params)) => {
            let ether = address_of(params, "ether").expect("ether parse error");
            create_ether_account::execute(config, &ether)
        }
        ("deposit", Some(params)) => {
            let amount = value_of(params, "amount").expect("amount parse error");
            let ether = address_of(params, "ether").expect("ether parse error");
            deposit::execute(config, amount, &ether)
        }
        ("get-ether-account-data", Some(params)) => {
            let ether = address_of(params, "ether").expect("ether parse error");
            get_ether_account_data::execute(config, &ether)
        }
        ("cancel-trx", Some(params)) => {
            let storage_account =
                pubkey_of(params, "storage_account").expect("storage_account parse error");
            cancel_trx::execute(config, &storage_account)
        }
        ("neon-elf-params", Some(params)) => {
            let program_location = params.value_of("program_location");
            get_neon_elf::execute(config, program_location)
        }
        ("collect-treasury", Some(_)) => collect_treasury::execute(config),
        ("init-environment", Some(params)) => {
            let file = params.value_of("file");
            let send_trx = params.is_present("send-trx");
            let force = params.is_present("force");
            let keys_dir = params.value_of("keys-dir");
            init_environment::execute(config, send_trx, force, keys_dir, file)
        }
        ("get-storage-at", Some(params)) => {
            let contract_id = address_of(params, "contract_id").expect("contract_it parse error");
            let index = u256_of(params, "index").expect("index parse error");
            get_storage_at::execute(config, contract_id, &index)
        }
        _ => unreachable!(),
    }
}

fn parse_tx(params: &ArgMatches) -> TxParams {
    let from = address_of(params, "sender").expect("sender parse error");
    let to = address_or_deploy_of(params, "contract");
    let data = read_stdin();
    let value = u256_of(params, "value");
    let gas_limit = u256_of(params, "gas_limit");

    TxParams {
        from,
        to,
        data,
        value,
        gas_limit,
    }
}

pub fn parse_tx_params(config: &Config, params: &ArgMatches) -> (Pubkey, u64, u64, Vec<Address>) {
    // Read ELF params only if token_mint or chain_id is not set.
    let mut token = pubkey_of(params, "token_mint");
    let mut chain = value_of(params, "chain_id");
    if token.is_none() || chain.is_none() {
        let cached_elf_params = CachedElfParams::new(config);
        token = token.or_else(|| {
            Some(
                Pubkey::from_str(
                    cached_elf_params
                        .get("NEON_TOKEN_MINT")
                        .expect("NEON_TOKEN_MINT load error"),
                )
                    .expect("NEON_TOKEN_MINT Pubkey ctor error "),
            )
        });
        chain = chain.or_else(|| {
            Some(
                u64::from_str(
                    cached_elf_params
                        .get("NEON_CHAIN_ID")
                        .expect("NEON_CHAIN_ID load error"),
                )
                    .expect("NEON_CHAIN_ID u64 ctor error"),
            )
        });
    }
    let token = token.expect("token_mint get error");
    let chain = chain.expect("chain_id get error");
    let max_steps =
        value_of::<u64>(params, "max_steps_to_execute").expect("max_steps_to_execute parse error");

    let accounts = values_of::<Address>(params, "cached_accounts").unwrap_or_default();

    (token, chain, max_steps, accounts)
}

fn parse_enable_return_data(params: &ArgMatches) -> bool {
    value_of(params, "enable_return_data").unwrap_or_default()
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

fn u256_of(matches: &ArgMatches<'_>, name: &str) -> Option<U256> {
    matches.value_of(name).map(|value| {
        if value.is_empty() {
            return U256::ZERO;
        }

        U256::from_str_prefixed(value).unwrap()
    })
}

fn read_stdin() -> Option<Vec<u8>> {
    let mut data = String::new();

    if let Ok(len) = std::io::stdin().read_line(&mut data) {
        if len == 0 {
            return None;
        }
        let data = truncate_0x(data.as_str());
        let bin = hex::decode(data).expect("data hex::decore error");
        Some(bin)
    } else {
        None
    }
}
