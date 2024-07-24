use std::fmt::Debug;

use ethnum::U256;
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

use crate::types::{vector::Vector, Address};

#[derive(Debug, Clone)]
#[repr(C)]
pub enum Action {
    ExternalInstruction {
        program_id: Pubkey,
        accounts: Vector<AccountMeta>,
        data: Vector<u8>,
        seeds: Vector<Vector<Vector<u8>>>,
        fee: u64,
        emulated_internally: bool,
    },
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
