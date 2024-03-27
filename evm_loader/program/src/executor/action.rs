use ethnum::U256;
use serde::{Deserialize, Serialize};
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

use crate::types::{serde::bytes_32, Address};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    ExternalInstruction {
        program_id: Pubkey,
        accounts: Vec<AccountMeta>,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        seeds: Vec<Vec<u8>>,
        fee: u64,
    },
    Transfer {
        source: Address,
        target: Address,
        chain_id: u64,
        #[serde(with = "ethnum::serde::bytes::le")]
        value: U256,
    },
    Burn {
        source: Address,
        chain_id: u64,
        #[serde(with = "ethnum::serde::bytes::le")]
        value: U256,
    },
    EvmSetStorage {
        address: Address,
        #[serde(with = "ethnum::serde::bytes::le")]
        index: U256,
        #[serde(with = "bytes_32")]
        value: [u8; 32],
    },
    EvmSetTransientStorage {
        address: Address,
        #[serde(with = "ethnum::serde::bytes::le")]
        index: U256,
        #[serde(with = "bytes_32")]
        value: [u8; 32],
    },
    EvmIncrementNonce {
        address: Address,
        chain_id: u64,
    },
    EvmSetCode {
        address: Address,
        chain_id: u64,
        #[serde(with = "serde_bytes")]
        code: Vec<u8>,
    },
}
