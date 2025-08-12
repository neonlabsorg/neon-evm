use crate::config::STATIC_STORAGE_LIMIT;
use crate::evm::precompile::is_precompile_address;
use crate::platform::{KeysIndex, FAKE_OPERATOR};
use crate::types::vector::VectorMapCell;
use crate::types::Address;
use allocator_api2::alloc::{self, Allocator};
use ethnum::U256;
use solana_program::pubkey::Pubkey;

use super::precompile_extension::is_precompile_extension;

pub struct TouchedAccounts<A: Allocator = alloc::Global> {
    touched_solana: VectorMapCell<Pubkey, u64, A>,
    touched_contracts: VectorMapCell<Address, u64, A>,
    touched_balances: VectorMapCell<(Address, u64), u64, A>,
    touched_storage: VectorMapCell<(Address, U256), u64, A>,
}

impl<A: Allocator + Copy> TouchedAccounts<A> {
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        Self {
            touched_solana: VectorMapCell::with_capacity_in(64, allocator),
            touched_contracts: VectorMapCell::with_capacity_in(16, allocator),
            touched_balances: VectorMapCell::with_capacity_in(32, allocator),
            touched_storage: VectorMapCell::with_capacity_in(64, allocator),
        }
    }
}

impl<A: Allocator> TouchedAccounts<A> {
    pub fn touch_balance(&self, address: Address, chain_id: u64) {
        Self::touch(&self.touched_balances, (address, chain_id), 2);
    }

    pub fn touch_balance_indirect(&self, address: Address, chain_id: u64) {
        Self::touch(&self.touched_balances, (address, chain_id), 1);
    }

    pub fn touch_contract(&self, address: Address) {
        if is_precompile_address(&address) || is_precompile_extension(&address) {
            return;
        }

        Self::touch(&self.touched_contracts, address, 2);
    }

    pub fn touch_storage(&self, address: Address, index: U256) {
        if index < STATIC_STORAGE_LIMIT {
            Self::touch(&self.touched_contracts, address, 2);
        } else {
            let index = index & !U256::new(0xFF);
            Self::touch(&self.touched_storage, (address, index), 2);
        }
    }

    pub fn touch_solana(&self, pubkey: Pubkey) {
        if pubkey == FAKE_OPERATOR {
            return;
        }

        Self::touch(&self.touched_solana, pubkey, 2);
    }

    fn touch<K: Ord>(map: &VectorMapCell<K, u64, A>, key: K, count: u64) {
        map.update_or_insert(key, count, |counter| *counter += count);
    }

    pub fn into_iter<K>(self, keys_index: &K) -> impl Iterator<Item = (Pubkey, u64)> + use<'_, K, A>
    where
        K: KeysIndex,
    {
        let solana = self.touched_solana.into_iter();

        let contracts = self.touched_contracts.into_iter().map(|(contract, count)| {
            let pubkey = keys_index.contract(contract);
            (pubkey, count)
        });

        let balances = self.touched_balances.into_iter().map(|(v, count)| {
            let pubkey = keys_index.balance(v.0, v.1);
            (pubkey, count)
        });

        let storage = self.touched_storage.into_iter().map(|(v, count)| {
            let pubkey = keys_index.storage(v.0, v.1);
            (pubkey, count)
        });

        solana.chain(contracts).chain(balances).chain(storage)
    }
}
