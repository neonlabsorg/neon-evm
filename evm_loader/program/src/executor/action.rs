use ethnum::U256;
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

use crate::types::{vector::{into_vector, Vector}, Address};

#[derive(Debug, Clone)]
pub enum Action {
    ExternalInstruction {
        program_id: Pubkey,
        accounts: Vector<AccountMeta>,
        data: Vector<u8>,
        seeds: Vector<Vector<u8>>,
        fee: u64,
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
    EvmIncrementNonce {
        address: Address,
        chain_id: u64,
    },
    EvmSetCode {
        address: Address,
        chain_id: u64,
        code: Vector<u8>,
    },
    EvmSelfDestruct {
        address: Address,
    },
}

pub fn filter_selfdestruct(actions: Vector<Action>) -> Vector<Action> {
    // Find all the account addresses which are scheduled to EvmSelfDestruct
    let accounts_to_destroy: std::collections::HashSet<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::EvmSelfDestruct { address } => Some(*address),
            _ => None,
        })
        .collect();

    // allocator_api2 does not implemented for Vector<T, Allocator>, hence we need an explicit copying...
    let tmp_actions = actions
        .into_iter()
        .filter(|action| {
            match action {
                // We always apply ExternalInstruction for Solana accounts
                // and NeonTransfer + NeonWithdraw
                Action::ExternalInstruction { .. }
                | Action::Transfer { .. }
                | Action::Burn { .. } => true,
                // We remove EvmSetStorage|EvmIncrementNonce|EvmSetCode if account is scheduled for destroy
                Action::EvmSetStorage { address, .. }
                | Action::EvmSetCode { address, .. }
                | Action::EvmIncrementNonce { address, .. } => {
                    !accounts_to_destroy.contains(address)
                }
                // SelfDestruct is only aplied to contracts deployed in the current transaction
                Action::EvmSelfDestruct { .. } => false,
            }
        })
        .collect();
    into_vector(tmp_actions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bincode() {
        let action = Action::EvmSetStorage {
            address: Address::default(),
            index: U256::from_le_bytes([
                255, 46, 185, 41, 144, 201, 3, 36, 227, 18, 148, 147, 106, 131, 110, 6, 229, 235,
                44, 154, 71, 124, 159, 144, 47, 119, 77, 5, 154, 49, 23, 54,
            ]),
            value: Default::default(),
        };
        //let serialized = bincode::serialize(&action).unwrap();
        //let _deserialized: Action = bincode::deserialize(&serialized).unwrap();
    }

    #[cfg(not(target_os = "solana"))]
    #[test]
    fn roundtrip_json() {
        let action = Action::EvmSetStorage {
            address: Address::default(),
            index: U256::from_le_bytes([
                255, 46, 185, 41, 144, 201, 3, 36, 227, 18, 148, 147, 106, 131, 110, 6, 229, 235,
                44, 154, 71, 124, 159, 144, 47, 119, 77, 5, 154, 49, 23, 54,
            ]),
            value: Default::default(),
        };
        //let serialized = serde_json::to_string(&action).unwrap();
        //let _deserialized: Action = serde_json::from_str(&serialized).unwrap();
    }
}
