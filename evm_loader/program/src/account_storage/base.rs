use std::collections::HashSet;

use crate::account::{
    AccountsDB, BalanceAccount, ContractAccount, Operator, StorageCell, Treasury,
};
use crate::account_storage::ProgramAccountStorage;
use crate::error::Result;
use crate::types::{Address, TrxView};
use ethnum::U256;
use solana_program::{clock::Clock, rent::Rent, sysvar::Sysvar};

use super::keys_cache::KeysCache;
use super::AccountStorage;

impl<'a> ProgramAccountStorage<'a> {
    pub fn new(accounts: AccountsDB<'a>) -> Result<Self> {
        Ok(Self {
            clock: Clock::get()?,
            rent: Rent::get()?,
            accounts,
            keys: KeysCache::new(),
            synced_modified_contracts: HashSet::new(),
        })
    }

    pub fn operator(&self) -> &Operator<'a> {
        self.accounts.operator()
    }

    pub fn treasury(&self) -> &Treasury<'a> {
        self.accounts.treasury()
    }

    pub fn db(&self) -> &AccountsDB<'a> {
        &self.accounts
    }

    pub fn storage_cell(&self, address: Address, index: U256) -> Result<StorageCell<'a>> {
        let pubkey = self.keys.storage_cell(&crate::ID, address, index);

        let account = self.accounts.get(&pubkey);
        StorageCell::from_account(&crate::ID, account.clone())
    }

    pub fn contract_account(&self, address: Address) -> Result<ContractAccount<'a>> {
        let pubkey = self.keys.contract(&crate::ID, address);

        let account = self.accounts.get(&pubkey);
        ContractAccount::from_account(&crate::ID, account.clone())
    }

    pub fn balance_account(&self, address: Address, chain_id: u64) -> Result<BalanceAccount<'a>> {
        let pubkey = self.keys.balance(&crate::ID, address, chain_id);

        let account = self.accounts.get(&pubkey);
        BalanceAccount::from_account(&crate::ID, account.clone())
    }

    pub fn create_balance_account(
        &self,
        address: Address,
        chain_id: u64,
    ) -> Result<BalanceAccount<'a>> {
        let account = BalanceAccount::create(
            address,
            chain_id,
            &self.accounts,
            Some(&self.keys),
            &self.rent,
        )?;

        Ok(account)
    }

    pub fn origin(
        &self,
        address: Address,
        transaction: &impl TrxView,
    ) -> Result<BalanceAccount<'a>> {
        let chain_id = transaction
            .chain_id()
            .unwrap_or_else(|| self.default_chain_id());
        self.create_balance_account(address, chain_id)
    }
}
