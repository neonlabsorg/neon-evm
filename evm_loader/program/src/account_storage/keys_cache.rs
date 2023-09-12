use std::{cell::RefCell, collections::HashMap};

use ethnum::U256;
use solana_program::pubkey::Pubkey;

use crate::{account::StorageCellAddress, types::Address};

type ContractKey = Address;
type BalanceKey = (Address, u64);
type StorageKey = (Address, U256);

pub struct KeysCache {
    contracts: RefCell<HashMap<ContractKey, (Pubkey, u8)>>,
    balances: RefCell<HashMap<BalanceKey, (Pubkey, u8)>>,
    storage_cells: RefCell<HashMap<StorageKey, Pubkey>>,
}

impl KeysCache {
    pub fn new() -> Self {
        Self {
            contracts: RefCell::new(HashMap::with_capacity(8)),
            balances: RefCell::new(HashMap::with_capacity(8)),
            storage_cells: RefCell::new(HashMap::with_capacity(32)),
        }
    }

    pub fn contract_with_bump_seed(&self, program_id: &Pubkey, address: Address) -> (Pubkey, u8) {
        *self
            .contracts
            .borrow_mut()
            .entry(address)
            .or_insert_with_key(|a| a.find_solana_address(program_id))
    }

    pub fn contract(&self, program_id: &Pubkey, address: Address) -> Pubkey {
        self.contract_with_bump_seed(program_id, address).0
    }

    pub fn balance(&self, program_id: &Pubkey, address: Address, chain_id: u64) -> Pubkey {
        self.balances
            .borrow_mut()
            .entry((address, chain_id))
            .or_insert_with_key(|(a, chain_id)| a.find_balance_address(program_id, *chain_id))
            .0
    }

    pub fn storage_cell(&self, program_id: &Pubkey, address: Address, index: U256) -> Pubkey {
        *self
            .storage_cells
            .borrow_mut()
            .entry((address, index))
            .or_insert_with(|| {
                let base = self.contract(program_id, address);
                let cell = StorageCellAddress::new(program_id, &base, &index);
                *cell.pubkey()
            })
    }
}
