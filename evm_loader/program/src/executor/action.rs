use std::fmt::Debug;

use ethnum::U256;
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

use crate::types::{boxx::Boxx, vector::Vector, Address};

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ExternalInstructionData {
    pub program_id: Pubkey,
    pub accounts: Vector<AccountMeta>,
    pub data: Vector<u8>,
    pub seeds: Vector<Vector<Vector<u8>>>,
    pub emulated_internally: bool,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub enum Action {
    ExternalInstruction(Boxx<ExternalInstructionData>),
    Transfer {
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    },
    Burn {
        source: Address,
        chain_id: u64,
        value: U256,
    },
    EvmSetStorage {
        address: Address,
        index: U256,
        value: [u8; 32],
    },
    EvmSetTransientStorage {
        address: Address,
        index: U256,
        value: [u8; 32],
    },
    EvmIncrementNonce {
        address: Address,
        chain_id: u64,
    },
    EvmSetCode {
        address: Address,
        chain_id: u64,
        code: Vector<u8>,
    },
}
