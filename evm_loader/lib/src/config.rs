use std::{env, str::FromStr};

use crate::types::RocksDbConfig;
use serde::{Deserialize, Serialize};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair};

const DEFAULT_ROCKSDB_PORT: u16 = 9888;

#[derive(Debug)]
pub struct Config {
    pub evm_loader: Pubkey,
    pub key_for_config: Pubkey,
    pub fee_payer: Option<Keypair>,
    pub commitment: CommitmentConfig,
    pub solana_cli_config: solana_cli_config::Config,
    pub db_config: Option<RocksDbConfig>,
    pub json_rpc_url: String,
    pub keypair_path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct APIOptions {
    pub solana_cli_config_path: Option<String>,
    pub commitment: CommitmentConfig,
    pub solana_url: String,
    pub solana_timeout: u64,
    pub solana_max_retries: usize,
    pub evm_loader: Pubkey,
    pub key_for_config: Pubkey,
    pub db_config: RocksDbConfig,
}

/// # Errors
#[must_use]
pub fn load_api_config_from_environment() -> APIOptions {
    let solana_cli_config_path: Option<String> =
        env::var("SOLANA_CLI_CONFIG_PATH").map(Some).unwrap_or(None);

    let commitment = env::var("COMMITMENT")
        .map(|v| v.to_lowercase())
        .ok()
        .and_then(|s| CommitmentConfig::from_str(&s).ok())
        .unwrap_or(CommitmentConfig::confirmed());

    let solana_url = env::var("SOLANA_URL").expect("solana url variable must be set");

    let solana_timeout = env::var("SOLANA_TIMEOUT").unwrap_or_else(|_| "30".to_string());
    let solana_timeout = solana_timeout
        .parse()
        .expect("SOLANA_TIMEOUT variable must be a valid number");

    let solana_max_retries = env::var("SOLANA_MAX_RETRIES").unwrap_or_else(|_| "10".to_string());
    let solana_max_retries = solana_max_retries
        .parse()
        .expect("SOLANA_MAX_RETRIES variable must be a valid number");

    let evm_loader = env::var("EVM_LOADER")
        .ok()
        .and_then(|v| Pubkey::from_str(&v).ok())
        .expect("EVM_LOADER variable must be a valid pubkey");

    let key_for_config = env::var("SOLANA_KEY_FOR_CONFIG")
        .ok()
        .and_then(|v| Pubkey::from_str(&v).ok())
        .expect("SOLANA_KEY_FOR_CONFIG variable must be a valid pubkey");

    let db_config = load_db_config_from_environment();

    APIOptions {
        solana_cli_config_path,
        commitment,
        solana_url,
        solana_timeout,
        solana_max_retries,
        evm_loader,
        key_for_config,
        db_config,
    }
}

fn load_db_config_from_environment() -> RocksDbConfig {
    let rocksdb_host = env::var("ROCKSDB_HOST")
        .as_deref()
        .unwrap_or("127.0.0.1")
        .to_owned();

    let rocksdb_port: u16 = env::var("ROCKSDB_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(DEFAULT_ROCKSDB_PORT);

    tracing::info!("rocksdb host {rocksdb_host}, port {rocksdb_port}");

    RocksDbConfig {
        rocksdb_host,
        rocksdb_port,
    }
}
