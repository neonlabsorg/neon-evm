use std::collections::HashMap;

use ethnum::U256;
use solana_program::pubkey::Pubkey;

use crate::{account::StorageCellAddress, types::Address};

type ContractKey = Address;
type BalanceKey = (Address, u64);
type StorageKey = (Address, U256);

type ContractMap = HashMap<ContractKey, (Pubkey, u8)>;
type BalanceMap = HashMap<BalanceKey, (Pubkey, u8)>;
type StorageMap = HashMap<StorageKey, StorageCellAddress>;

pub struct KeysCache {
    contracts: ContractMap,
    balances: BalanceMap,
    storage_cells: StorageMap,
}

fn contract_with_bump_seed_mut<'a>(
    contracts: &'a mut ContractMap,
    program_id: &Pubkey,
    address: Address,
) -> &'a mut (Pubkey, u8) {
    contracts
        .entry(address)
        .or_insert_with_key(|address| address.find_solana_address(program_id))
}

fn balance_with_bump_seed<'a>(
    balances: &'a mut BalanceMap,
    program_id: &Pubkey,
    address: Address,
    chain_id: u64,
) -> &'a mut (Pubkey, u8) {
    balances
        .entry((address, chain_id))
        .or_insert_with_key(|(address, chain_id)| {
            address.find_balance_address(program_id, *chain_id)
        })
}

impl KeysCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            contracts: HashMap::with_capacity(8),
            balances: HashMap::with_capacity(8),
            storage_cells: HashMap::with_capacity(32),
        }
    }

    #[must_use]
    pub fn contract_with_bump_seed(
        &mut self,
        program_id: &Pubkey,
        address: Address,
    ) -> (Pubkey, u8) {
        *contract_with_bump_seed_mut(&mut self.contracts, program_id, address)
    }

    #[must_use]
    pub fn contract(&mut self, program_id: &Pubkey, address: Address) -> Pubkey {
        self.contract_with_bump_seed(program_id, address).0
    }

    #[must_use]
    pub fn balance_with_bump_seed(
        &mut self,
        program_id: &Pubkey,
        address: Address,
        chain_id: u64,
    ) -> (Pubkey, u8) {
        *balance_with_bump_seed(&mut self.balances, program_id, address, chain_id)
    }

    #[must_use]
    pub fn balance(&mut self, program_id: &Pubkey, address: Address, chain_id: u64) -> Pubkey {
        self.balance_with_bump_seed(program_id, address, chain_id).0
    }

    #[must_use]
    pub fn storage_cell(&mut self, program_id: &Pubkey, address: Address, index: U256) -> Pubkey {
        *self
            .storage_cell_address(program_id, address, index)
            .pubkey()
    }

    #[must_use]
    pub fn storage_cell_address(
        &mut self,
        program_id: &Pubkey,
        address: Address,
        index: U256,
    ) -> StorageCellAddress {
        *self
            .storage_cells
            .entry((address, index))
            .or_insert_with(|| {
                let base = contract_with_bump_seed_mut(&mut self.contracts, program_id, address).0;
                StorageCellAddress::new(program_id, &base, &index)
            })
    }
}

impl Default for KeysCache {
    fn default() -> Self {
        Self::new()
    }
}
