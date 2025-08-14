#![allow(unused_macros, unused_imports)]

use crate::{
    config::{ACCOUNT_SEED_VERSION, TREASURY_POOL_SEED},
    types::Address,
};
use ethnum::U256;
use solana_program::pubkey::Pubkey;

// Program Derived Addresses for all account types in the program.
// Caution: When adding new account types, make sure no collisions occur with existing seeds.
//          Using a unique prefix for each account type is recommended.

#[must_use]
pub fn main_treasury_pool_address(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY_POOL_SEED.as_bytes()], program_id)
}

#[must_use]
pub fn aux_treasury_pool_address(program_id: &Pubkey, index: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[TREASURY_POOL_SEED.as_bytes(), &index.to_le_bytes()],
        program_id,
    )
}

#[must_use]
pub fn tree_account(
    program_id: &Pubkey,
    payer: &Address,
    chain_id: u64,
    nonce: u64,
) -> (Pubkey, u8) {
    let seeds: &[&[u8]] = &[
        &[ACCOUNT_SEED_VERSION],
        b"TREE",
        payer.as_bytes(),
        &chain_id.to_le_bytes(),
        &nonce.to_le_bytes(),
    ];

    Pubkey::find_program_address(seeds, program_id)
}

macro_rules! tree_account_seeds {
    ($init:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            b"TREE",
            ($init).payer.as_bytes(),
            &($init).chain_id.to_le_bytes(),
            &($init).nonce.to_le_bytes(),
            &[$bump_seed],
        ]
    };
}
pub(crate) use tree_account_seeds;

#[must_use]
pub fn main_pool_authority(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"Deposit"], program_id)
}

macro_rules! main_pool_authority_seeds {
    ($bump_seed:expr) => {
        &[b"Deposit", &[$bump_seed]]
    };
}
pub(crate) use main_pool_authority_seeds;

#[must_use]
pub fn operator_balance(
    program_id: &Pubkey,
    operator: &Pubkey,
    balance: &Address,
    chain_id: u64,
) -> (Pubkey, u8) {
    let chain_id = U256::from(chain_id);
    let operator_seeds: &[&[u8]] = &[
        &[ACCOUNT_SEED_VERSION],
        operator.as_ref(),
        balance.as_bytes(),
        &chain_id.to_be_bytes(),
    ];
    Pubkey::find_program_address(operator_seeds, program_id)
}

macro_rules! operator_balance_seeds {
    ($operator:expr, $balance:expr, $chain_id:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            $operator.as_ref(),
            $balance.as_bytes(),
            &U256::from($chain_id).to_be_bytes(),
            &[$bump_seed],
        ]
    };
}
pub(crate) use operator_balance_seeds;

#[must_use]
pub fn balance(program_id: &Pubkey, account: &Address, chain_id: u64) -> (Pubkey, u8) {
    let chain_id = U256::from(chain_id);
    let balance_seeds: &[&[u8]] = &[
        &[ACCOUNT_SEED_VERSION],
        account.as_bytes(),
        &chain_id.to_be_bytes(),
    ];
    Pubkey::find_program_address(balance_seeds, program_id)
}

macro_rules! balance_seeds {
    ($account:expr, $chain_id:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            $account.as_bytes(),
            &U256::from($chain_id).to_be_bytes(),
            &[$bump_seed],
        ]
    };
}
pub(crate) use balance_seeds;

#[must_use]
pub fn contract(program_id: &Pubkey, contract: &Address) -> (Pubkey, u8) {
    let contract_seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], contract.as_bytes()];
    Pubkey::find_program_address(contract_seeds, program_id)
}

macro_rules! contract_seeds {
    ($contract:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            $contract.as_bytes(),
            &[$bump_seed],
        ]
    };
}
pub(crate) use contract_seeds;

#[must_use]
pub fn contract_payer(program_id: &Pubkey, contract: &Address) -> (Pubkey, u8) {
    let payer_seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"PAYER", contract.as_bytes()];
    Pubkey::find_program_address(payer_seeds, program_id)
}

macro_rules! contract_payer_seeds {
    ($contract:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            b"PAYER",
            $contract.as_bytes(),
            &[$bump_seed],
        ]
    };
}
pub(crate) use contract_payer_seeds;

#[must_use]
pub fn contract_auth(program_id: &Pubkey, contract: &Address, salt: &[u8; 32]) -> (Pubkey, u8) {
    let auth_seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"AUTH", contract.as_bytes(), salt];
    Pubkey::find_program_address(auth_seeds, program_id)
}

macro_rules! contract_auth_seeds {
    ($contract:expr, $salt:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            b"AUTH",
            $contract.as_bytes(),
            $salt,
            &[$bump_seed],
        ]
    };
}
pub(crate) use contract_auth_seeds;

#[must_use]
pub fn contract_data(program_id: &Pubkey, contract: &Address, salt: &[u8; 32]) -> (Pubkey, u8) {
    let data_seeds: &[&[u8]] = &[
        &[ACCOUNT_SEED_VERSION],
        b"ContractData",
        contract.as_bytes(),
        salt,
    ];
    Pubkey::find_program_address(data_seeds, program_id)
}

macro_rules! contract_data_seeds {
    ($contract:expr, $salt:expr, $bump_seed:expr) => {
        &[
            &[$crate::config::ACCOUNT_SEED_VERSION],
            b"ContractData",
            $contract.as_bytes(),
            $salt,
            &[$bump_seed],
        ]
    };
}
pub(crate) use contract_data_seeds;
