use crate::{
    account::{pda, StorageCellSeed},
    types::{vector::VectorMapCell, Address},
};
use allocator_api2::alloc;
use ethnum::U256;
use solana_program::pubkey::Pubkey;

pub trait KeysIndex {
    fn balance_bump(&self, address: Address, chain_id: u64) -> (Pubkey, u8);
    fn balance(&self, address: Address, chain_id: u64) -> Pubkey;

    fn contract_bump(&self, address: Address) -> (Pubkey, u8);
    fn contract(&self, address: Address) -> Pubkey;

    fn storage(&self, contract: Address, index: U256) -> Pubkey;
}

pub struct DefaultKeysIndex {
    program_id: Pubkey,
}

impl DefaultKeysIndex {
    #[must_use]
    pub fn new(program_id: Pubkey) -> Self {
        Self { program_id }
    }
}

impl KeysIndex for DefaultKeysIndex {
    #[inline]
    fn balance(&self, address: Address, chain_id: u64) -> Pubkey {
        pda::balance_address(&self.program_id, &address, chain_id).0
    }

    #[inline]
    fn balance_bump(&self, address: Address, chain_id: u64) -> (Pubkey, u8) {
        pda::balance_address(&self.program_id, &address, chain_id)
    }

    #[inline]
    fn contract(&self, address: Address) -> Pubkey {
        pda::contract_address(&self.program_id, &address).0
    }

    #[inline]
    fn contract_bump(&self, address: Address) -> (Pubkey, u8) {
        pda::contract_address(&self.program_id, &address)
    }

    #[inline]
    fn storage(&self, contract: Address, index: U256) -> Pubkey {
        let base = self.contract(contract);
        let storage_seed = StorageCellSeed::new(index);
        Pubkey::create_with_seed(&base, &storage_seed, &self.program_id).unwrap()
    }
}

pub struct CachedKeysIndex {
    program_id: Pubkey,
    balance_cache: VectorMapCell<(Address, u64), (Pubkey, u8), alloc::Global>,
    contract_cache: VectorMapCell<Address, (Pubkey, u8), alloc::Global>,
}

impl CachedKeysIndex {
    #[must_use]
    pub fn new(program_id: Pubkey) -> Self {
        Self {
            program_id,
            balance_cache: VectorMapCell::with_capacity(64),
            contract_cache: VectorMapCell::with_capacity(32),
        }
    }
}

impl KeysIndex for CachedKeysIndex {
    #[inline]
    fn balance(&self, address: Address, chain_id: u64) -> Pubkey {
        self.balance_bump(address, chain_id).0
    }

    fn balance_bump(&self, address: Address, chain_id: u64) -> (Pubkey, u8) {
        self.balance_cache
            .get_or_insert((address, chain_id), |(address, chain_id)| {
                pda::balance_address(&self.program_id, address, *chain_id)
            })
    }

    #[inline]
    fn contract(&self, address: Address) -> Pubkey {
        self.contract_bump(address).0
    }

    fn contract_bump(&self, address: Address) -> (Pubkey, u8) {
        self.contract_cache.get_or_insert(address, |address| {
            pda::contract_address(&self.program_id, address)
        })
    }

    #[inline]
    fn storage(&self, contract: Address, index: U256) -> Pubkey {
        let base = self.contract(contract);
        let storage_seed = StorageCellSeed::new(index);
        Pubkey::create_with_seed(&base, &storage_seed, &self.program_id).unwrap()
    }
}
