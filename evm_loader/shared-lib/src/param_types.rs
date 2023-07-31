use ethnum::U256;
use evm_loader::types::Address;
use neon_lib::{
    config::APIOptions,
    types::{trace::TraceCallConfig, TxParams},
};
use serde::Deserialize;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

#[derive(Deserialize)]
pub struct Params<T> {
    pub api_options: APIOptions,
    pub slot: Option<u64>,
    pub params: T,
}

#[derive(Deserialize)]
pub struct CancelTrx {
    pub storage_account: Pubkey,
}

#[derive(Deserialize)]
pub struct CreateEtherAccount {
    pub ether_address: Address,
}

#[derive(Deserialize)]
pub struct Deposit {
    pub amount: u64,
    pub ether_address: Address,
}

#[derive(Deserialize)]
pub struct Emulate {
    pub tx_params: TxParams,
    pub token_mint: Pubkey,
    pub chain_id: u64,
    pub step_limit: u64,
    pub commitment: CommitmentConfig,
    pub accounts: Vec<Address>,
    pub solana_accounts: Vec<Pubkey>,
    pub trace_call_config: TraceCallConfig,
}

#[derive(Deserialize)]
pub struct GetEtherAccountData {
    pub ether_address: Address,
}

#[derive(Deserialize)]
pub struct GetNeonElf {
    pub program_location: Option<String>,
}

#[derive(Deserialize)]
pub struct GetStorageAt {
    pub ether_address: Address,
    pub index: U256,
}
