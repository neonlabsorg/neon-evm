use clap::{crate_description, crate_name, App, AppSettings, Arg, ArgMatches, SubCommand};
use ethnum::U256;
use evm_loader::types::Address;
use solana_clap_utils::input_validators::{is_url_or_moniker, is_valid_pubkey};
use std::fmt::Display;

// Return an error if string cannot be parsed as a Address address
fn is_valid_address<T>(string: T) -> Result<(), String>
where
    T: AsRef<str>,
{
    Address::from_hex(string.as_ref())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Return an error if string cannot be parsed as a Address address
fn is_valid_address_or_deploy<T>(string: T) -> Result<(), String>
where
    T: AsRef<str>,
{
    if string.as_ref() == "deploy" {
        return Ok(());
    }
    Address::from_hex(string.as_ref())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Return an error if string cannot be parsed as a U256 integer
fn is_valid_u256<T>(string: T) -> Result<(), String>
where
    T: AsRef<str>,
{
    let value = string.as_ref();
    if value.is_empty() {
        return Ok(());
    }

    U256::from_str_prefixed(value)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn is_amount<T, U>(amount: U) -> Result<(), String>
where
    T: std::str::FromStr,
    U: AsRef<str> + Display,
{
    if amount.as_ref().parse::<T>().is_ok() {
        Ok(())
    } else {
        Err(format!(
            "Unable to parse argument as {}, provided: {amount}",
            std::any::type_name::<T>()
        ))
    }
}

fn ether_arg<'a, 'b>(idx: u64) -> Arg<'a, 'b> {
    Arg::with_name("ether")
        .index(idx)
        .value_name("ETHER")
        .takes_value(true)
        .required(true)
        .validator(is_valid_address)
        .help("Ethereum address")
}

fn token_mint_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name("token_mint")
        .long("token_mint")
        .value_name("TOKEN_MINT")
        .takes_value(true)
        .global(true)
        .validator(is_valid_pubkey)
        .help("Pubkey for token_mint")
}

fn chain_id_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name("chain_id")
        .long("chain_id")
        .value_name("CHAIN_ID")
        .takes_value(true)
        .required(false)
        .help("Network chain_id")
}

fn max_steps_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name("max_steps_to_execute")
        .long("max_steps_to_execute")
        .value_name("NUMBER_OF_STEPS")
        .takes_value(true)
        .required(false)
        .default_value("100000")
        .help("Maximal number of steps to execute in a single run")
}

fn trx_params<'a, 'b>(cmd: &'static str, desc: &'static str) -> App<'a, 'b> {
    SubCommand::with_name(cmd)
        .about(desc)
        .arg(
            Arg::with_name("sender")
                .value_name("SENDER")
                .takes_value(true)
                .index(1)
                .required(true)
                .validator(is_valid_address)
                .help("The sender of the transaction"),
        )
        .arg(
            Arg::with_name("contract")
                .value_name("CONTRACT")
                .takes_value(true)
                .index(2)
                .required(true)
                .validator(is_valid_address_or_deploy)
                .help("The contract that executes the transaction or 'deploy'"),
        )
        .arg(
            Arg::with_name("value")
                .value_name("VALUE")
                .takes_value(true)
                .index(3)
                .required(false)
                .validator(is_valid_u256)
                .help("Transaction value"),
        )
        .arg(token_mint_arg())
        .arg(chain_id_arg())
        .arg(max_steps_arg())
        .arg(
            Arg::with_name("gas_limit")
                .short("G")
                .long("gas_limit")
                .value_name("GAS_LIMIT")
                .takes_value(true)
                .required(false)
                .validator(is_valid_u256)
                .help("Gas limit"),
        )
        .arg(
            Arg::with_name("access_list")
                .long("access-list")
                .takes_value(true)
                .required(false)
                .multiple(true)
                .value_name("ADDRESS [STORAGE_KEYS ...]"),
        )
        .arg(
            Arg::with_name("cached_accounts")
                .value_name("CACHED_ACCOUNTS")
                .long("cached_accounts")
                .takes_value(true)
                .required(false)
                .multiple(true)
                .validator(is_valid_address)
                .help("List of cached account addresses"),
        )
        .arg(
            Arg::with_name("solana_accounts")
                .value_name("SOLANA_ACCOUNTS")
                .long("solana_accounts")
                .takes_value(true)
                .required(false)
                .multiple(true)
                .validator(is_valid_address)
                .help("List of cached solana account pubkeys"),
        )
}

#[allow(clippy::too_many_lines)]
pub fn parse<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
        .about(crate_description!())
        .version(concat!("Neon-cli/v", env!("CARGO_PKG_VERSION"), "-", env!("NEON_REVISION")))
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg({
            let arg = Arg::with_name("config_file")
                .short("C")
                .long("config")
                .value_name("PATH")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use");

            #[allow(clippy::option_if_let_else)]
            if let Some(ref config_file) = *solana_cli_config::CONFIG_FILE {
                arg.default_value(config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::with_name("db_config")
                .long("db_config")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use Tracer DB")
        )
        .arg(
            Arg::with_name("slot")
                .short("L")
                .long("slot")
                .value_name("slot")
                .takes_value(true)
                .required(false)
                .validator(is_amount::<u64, _>)
                .help("Slot number to work with archived data"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .takes_value(false)
                .global(true)
                .multiple(true)
                .help("Increase message verbosity"),
        )
        .arg(
            Arg::with_name("fee_payer")
                .long("fee-payer")
                .takes_value(true)
                .global(true)
                .help("Specify fee payer for transactions (use default solana account if not specified)")
        )
        .arg(
            Arg::with_name("keypair")
                .long("keypair")
                .takes_value(true)
                .global(true)
                .help("Specify signer for transactions (use default solana account if not specified)")
        )
        .arg(
            Arg::with_name("json_rpc_url")
                .short("u")
                .long("url")
                .value_name("URL")
                .takes_value(true)
                .global(true)
                .validator(is_url_or_moniker)
                .help("URL for Solana node"),
        )
        .arg(
            Arg::with_name("evm_loader")
                .long("evm_loader")
                .value_name("EVM_LOADER")
                .takes_value(true)
                .global(true)
                .validator(is_valid_pubkey)
                .help("Pubkey for evm_loader contract")
        )
        .arg(
            Arg::with_name("commitment")
                .long("commitment")
                .takes_value(true)
                .possible_values(&[
                    "processed",
                    "confirmed",
                    "finalized",
                    "recent", // Deprecated as of v1.5.5
                    "single", // Deprecated as of v1.5.5
                    "singleGossip", // Deprecated as of v1.5.5
                    "root", // Deprecated as of v1.5.5
                    "max", // Deprecated as of v1.5.5
                ])
                .value_name("COMMITMENT_LEVEL")
                .hide_possible_values(true)
                .global(true)
                .default_value("finalized")
                .help("Return information at the selected commitment level [possible values: processed, confirmed, finalized]"),
        )
        .arg(
            Arg::with_name("loglevel")
                .short("l")
                .long("loglevel")
                .value_name("LOG_LEVEL")
                .takes_value(true)
                .global(true)
                .help("Logging level"),
        )
        .subcommand(
            trx_params(
                "emulate",
                "Emulation transaction. Additional `TransactionParams` can be provided via STDIN as a JSON object.",
            )
        )
        .subcommand(
            trx_params(
                "trace",
                "Emulation transaction to collecting traces. Additional `TransactionParams` can be provided via STDIN as a JSON object.",
            )
        )
        .subcommand(
            SubCommand::with_name("create-ether-account")
                .about("Create ethereum account")
                .arg(ether_arg(1))
        )
        .subcommand(
            SubCommand::with_name("deposit")
                .about("Deposit NEONs to ether account")
                .arg(
                    Arg::with_name("amount")
                        .index(1)
                        .value_name("AMOUNT")
                        .takes_value(true)
                        .required(true)
                        .validator(is_amount::<u64, _>)
                        .help("Amount to deposit"),
                )
                .arg(ether_arg(2))
        )
        .subcommand(
            SubCommand::with_name("get-ether-account-data")
                .about("Get values stored in associated with given address account data")
                .arg(ether_arg(1))
        )
        .subcommand(
            SubCommand::with_name("cancel-trx")
                .about("Cancel NEON transaction")
                .arg(
                    Arg::with_name("storage_account")
                        .index(1)
                        .value_name("STORAGE_ACCOUNT")
                        .takes_value(true)
                        .required(true)
                        .validator(is_valid_pubkey)
                        .help("storage account for transaction"),
                )
        )
        .subcommand(
            SubCommand::with_name("neon-elf-params")
                .about("Get NEON values stored in elf")
                .arg(
                    Arg::with_name("program_location")
                        .index(1)
                        .value_name("PROGRAM_FILEPATH")
                        .takes_value(true)
                        .required(false)
                        .help("/path/to/evm_loader.so"),
                )
        )
        .subcommand(
            SubCommand::with_name("collect-treasury")
                .about("Collect lamports from auxiliary treasury accounts to the main treasury balance")
        )
        .subcommand(
            SubCommand::with_name("init-environment")
                .about("Initialize and verify environment for NeonEVM execution")
                .arg(
                    Arg::with_name("send-trx")
                        .long("send-trx")
                        .takes_value(false)
                        .help("Send transaction for initialize"),
                )
                .arg(
                    Arg::with_name("force")
                        .long("force")
                        .takes_value(false)
                        .help("Force initialize environment (even if NeonEVM and CLI version mismatch)"),
                )
                .arg(
                    Arg::with_name("keys-dir")
                        .long("keys-dir")
                        .takes_value(true)
                        .help("Directory with private-keys")
                )
                .arg(
                    Arg::with_name("file")
                        .index(1)
                        .value_name("FILE")
                        .takes_value(true)
                        .required(false)
                        .help("Path to file with program image /path/to/evm_loader.so"),
                )
        )
        .subcommand(
            SubCommand::with_name("get-storage-at")
                .about("Get Ethereum storage value at given index")
                .arg(
                    Arg::with_name("contract_id")
                        .index(1)
                        .value_name("contract_id")
                        .takes_value(true)
                        .validator(is_valid_address)
                        .required(true),
                )
                .arg(
                    Arg::with_name("index")
                        .index(2)
                        .value_name("index")
                        .takes_value(true)
                        .validator(is_valid_u256)
                        .required(true),
                )
        )
        .get_matches()
}
