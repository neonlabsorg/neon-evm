use crate::{
    account::pda,
    types::{btree_map_cell::BTreeMapCell, Address},
};
use solana_program::pubkey::Pubkey;

pub trait KeysIndex {
    fn balance_bump(&self, address: Address, chain_id: u64) -> (Pubkey, u8);
    fn balance(&self, address: Address, chain_id: u64) -> Pubkey;
    fn contract_bump(&self, address: Address) -> (Pubkey, u8);
    fn contract(&self, address: Address) -> Pubkey;
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
}

pub struct CachedKeysIndex {
    program_id: Pubkey,
    balance_cache: BTreeMapCell<(Address, u64), (Pubkey, u8)>,
    contract_cache: BTreeMapCell<Address, (Pubkey, u8)>,
}

impl CachedKeysIndex {
    #[must_use]
    pub fn new(program_id: Pubkey) -> Self {
        Self {
            program_id,
            balance_cache: BTreeMapCell::new(),
            contract_cache: BTreeMapCell::new(),
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
}
