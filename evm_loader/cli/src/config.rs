use clap::ArgMatches;
pub use neon_lib::config::*;
use neon_lib::NeonCliError;
use solana_clap_utils::{
    input_parsers::pubkey_of, input_validators::normalize_to_url_if_moniker,
    keypair::keypair_from_path,
};
use solana_sdk::commitment_config::CommitmentConfig;
use std::str::FromStr;

/// # Panics
/// # Errors
/// `EvmLoaderNotSpecified` - if `evm_loader` is not specified
/// `KeypairNotSpecified` - if `signer` is not specified
pub fn create(options: &ArgMatches) -> Result<Config, NeonCliError> {
    let solana_cli_config = options
        .value_of("config_file")
        .map_or_else(solana_cli_config::Config::default, |config_file| {
            solana_cli_config::Config::load(config_file).unwrap_or_default()
        });

    let commitment =
        CommitmentConfig::from_str(options.value_of("commitment").unwrap_or("confirmed")).unwrap();

    let json_rpc_url = normalize_to_url_if_moniker(
        options
            .value_of("json_rpc_url")
            .unwrap_or(&solana_cli_config.json_rpc_url),
    );

    let evm_loader = if let Some(value) = pubkey_of(options, "evm_loader") {
        value
    } else {
        return Err(NeonCliError::EvmLoaderNotSpecified);
    };

    let keypair_path: String = options
        .value_of("keypair")
        .unwrap_or(&solana_cli_config.keypair_path)
        .to_owned();

    let fee_payer = keypair_from_path(
        options,
        options
            .value_of("fee_payer")
            .unwrap_or(&solana_cli_config.keypair_path),
        "fee_payer",
        true,
    )
    .ok();

    let db_config = options
        .value_of("db_config")
        .map(|path| solana_cli_config::load_config_file(path).expect("load db-config error"));

    Ok(Config {
        evm_loader,
        fee_payer,
        commitment,
        solana_cli_config,
        db_config,
        json_rpc_url,
        keypair_path,
    })
}
