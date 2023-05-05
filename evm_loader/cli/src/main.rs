#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic)]

mod logs;
mod program_options;

use clap::ArgMatches;
pub use config::Config;

use ethnum::U256;
use serde_json::json;
use solana_clap_utils::input_parsers::{pubkey_of, value_of, values_of};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use evm_loader::types::Address;
use neon_cli::{commands::{
    get_neon_elf::CachedElfParams, cancel_trx, collect_treasury, create_ether_account, deposit,
    emulate, get_ether_account_data, get_neon_elf, get_storage_at, init_environment, trace,
}, rpc::Rpc, types::{
    trace::{TraceCallConfig, TraceConfig},
    TraceBlockBySlotParams, TransactionHashParams, TransactionParams, TxParams,
}, config, NeonCliResult, NeonCliError};

fn run(options: &ArgMatches) -> NeonCliResult {
    let (cmd, params) = options.subcommand();
    let config = config::create(options)?;

    execute(cmd, params, &config)
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

#[tokio::main]
async fn main() {
    let options = program_options::parse();

    logs::init(&options).expect("logs init error");
    std::panic::set_hook(Box::new(|info| {
        let message = std::format!("Panic: {info}");
        print_result(&Err(NeonCliError::Panic(message)));
    }));

    let result = run(&options);

    print_result(&result);
    if let Err(e) = result {
        std::process::exit(e.error_code());
    };
}

#[allow(clippy::too_many_lines)]
fn execute(cmd: &str, params: Option<&ArgMatches>, config: &Config) -> NeonCliResult {
    match (cmd, params) {
        ("emulate", Some(params)) => {
            let (tx, trace_call_config) = parse_tx(params);
            let (token, chain, steps, accounts, solana_accounts) = parse_tx_params(config, params);
            emulate::execute(
                config.rpc_client.as_ref(),
                config.evm_loader,
                tx,
                token,
                chain,
                steps,
                config.commitment,
                &accounts,
                &solana_accounts,
                trace_call_config,
            ).map(|result| json!(result))
        }
        ("emulate-hash", Some(params)) => {
            let (tx, trace_config) = parse_tx_hash(config.rpc_client.as_ref());
            let (token, chain, steps, accounts, solana_accounts, ) = parse_tx_params(config, params);
            emulate::execute(
                config.rpc_client.as_ref(),
                config.evm_loader,
                tx,
                token,
                chain,
                steps,
                config.commitment,
                &accounts,
                &solana_accounts,
                trace_config.into(),
            ).map(|result| json!(result))
        }
        ("trace", Some(params)) => {
            let (tx, trace_call_config) = parse_tx(params);
            let (token, chain, steps, accounts, solana_accounts) = parse_tx_params(config, params);
            trace::trace_transaction(
                config.rpc_client.as_ref(),
                config.evm_loader,
                tx,
                token,
                chain,
                steps,
                config.commitment,
                &accounts,
                &solana_accounts,
                trace_call_config,
            ).map(|trace| json!(trace))
        }
        ("trace-hash", Some(params)) => {
            let (tx, trace_config) = parse_tx_hash(config.rpc_client.as_ref());
            let (token, chain, steps, accounts, solana_accounts) = parse_tx_params(config, params);
            trace::trace_transaction(
                config.rpc_client.as_ref(),
                config.evm_loader,
                tx,
                token,
                chain,
                steps,
                config.commitment,
                &accounts,
                &solana_accounts,
                trace_config.into(),
            ).map(|trace| json!(trace))
        }
        ("trace-block-by-slot", Some(params)) => {
            let slot = params.value_of("slot").expect("SLOT argument is not provided");
            let slot: u64 = slot.parse().expect("slot parse error");
            let trace_block_params: Option<TraceBlockBySlotParams> = serde_json::from_reader(std::io::BufReader::new(std::io::stdin()))
                .unwrap_or_else(|err| panic!("Unable to parse `TraceBlockBySlotParams` from STDIN, error: {err:?}"));
            let trace_config = trace_block_params
                .map(|params| params.trace_config.unwrap_or_default())
                .unwrap_or_default();
            let (token, chain, steps, accounts, solana_accounts) = parse_tx_params(config, params);
            let transactions = config.rpc_client.get_block_transactions(slot)?;
            trace::trace_block(
                config.rpc_client.as_ref(),
                config.evm_loader,
                transactions,
                token,
                chain,
                steps, config.commitment,
                &accounts,
                &solana_accounts,
                trace_config,
            ).map(|traces| json!(traces))
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
            get_ether_account_data::execute(config.rpc_client.as_ref(), &config.evm_loader, &ether)
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
            get_storage_at::execute(config.rpc_client.as_ref(), &config.evm_loader, contract_id, &index)
                .map(|hash| json!(hex::encode(hash)))
        }
        _ => unreachable!(),
    }
}

fn parse_tx(params: &ArgMatches) -> (TxParams, TraceCallConfig) {
    let from = address_of(params, "sender").expect("sender parse error");
    let to = address_or_deploy_of(params, "contract");
    let transaction_params: Option<TransactionParams> = serde_json::from_reader(std::io::BufReader::new(std::io::stdin()))
        .unwrap_or_else(|err| panic!("Unable to parse `TransactionParams` from STDIN, error: {err:?}"));
    let (data, trace_config) = transaction_params
        .map(|params| (params.data.map(Into::into), params.trace_config.unwrap_or_default()))
        .unwrap_or_default();
    let value = u256_of(params, "value");
    let gas_limit = u256_of(params, "gas_limit");

    let tx_params = TxParams {
        nonce: None,
        from,
        to,
        data,
        value,
        gas_limit,
    };

    (tx_params, trace_config)
}

fn parse_tx_hash(rpc_client: &dyn Rpc) -> (TxParams, TraceConfig) {
    let tx = rpc_client.get_transaction_data().unwrap();
    let transaction_params: Option<TransactionHashParams> = serde_json::from_reader(std::io::BufReader::new(std::io::stdin()))
        .unwrap_or_else(|err| panic!("Unable to parse `TransactionHashParams` from STDIN, error: {err:?}"));

    let trace_config = transaction_params
        .map(|params| params.trace_config.unwrap_or_default())
        .unwrap_or_default();

    (tx, trace_config)
}

#[must_use]
pub fn parse_tx_params(config: &Config, params: &ArgMatches) -> (Pubkey, u64, u64, Vec<Address>, Vec<Pubkey>) {
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
    let solana_accounts = values_of::<Pubkey>(params, "solana_accounts").unwrap_or_default();

    (token, chain, max_steps, accounts, solana_accounts)
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
