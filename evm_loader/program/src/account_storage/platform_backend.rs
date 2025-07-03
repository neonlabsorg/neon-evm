use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{clock::Clock, instruction::Instruction, pubkey::Pubkey, rent::Rent};

use crate::{
    account::{pda, Account, StorageCellSeed},
    account_storage::{AccountStorage, LogCollector, SyncedAccountStorage},
    error::Result,
    executor::OwnedAccountInfo,
    platform::{Platform, FAKE_OPERATOR},
    types::{vector::VectorSliceExt, Address, Vector},
    vector,
};

#[maybe_async(?Send)]
impl<'a, T: Platform<'a>> LogCollector for T {
    async fn collect_log<const N: usize>(
        &mut self,
        address: &[u8; 20],
        topics: [[u8; 32]; N],
        data: &[u8],
    ) {
        let address = Address(*address);
        self.log_event(address, topics, data).await;
    }
}

#[maybe_async(?Send)]
impl<'a, T: Platform<'a>> AccountStorage for T {
    fn program_id(&self) -> Pubkey {
        self.program_id()
    }

    fn operator(&self) -> Pubkey {
        self.operator()
    }

    async fn block_number(&self) -> U256 {
        let clock: Clock = self.get_sysvar().await.unwrap();
        clock.slot.into()
    }

    async fn block_timestamp(&self) -> U256 {
        let clock: Clock = self.get_sysvar().await.unwrap();
        clock
            .unix_timestamp
            .try_into()
            .expect("Timestamp is positive")
    }

    async fn block_hash(&self, slot: u64) -> [u8; 32] {
        super::block_hash::find_slot_hash(slot)
    }

    async fn rent(&self) -> Rent {
        self.get_sysvar().await.unwrap()
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.get_return_data()
    }

    fn set_return_data(&mut self, _data: &[u8]) {
        // do nothing
    }

    async fn nonce(&self, address: Address, chain_id: u64) -> u64 {
        self.get_balance(address, chain_id)
            .await
            .unwrap()
            .map_or(0_u64, |a| a.nonce())
    }

    async fn balance(&self, address: Address, chain_id: u64) -> U256 {
        self.get_balance(address, chain_id)
            .await
            .unwrap()
            .map_or(U256::ZERO, |a| a.balance())
    }

    async fn solana_user_address(&self, address: Address) -> Option<Pubkey> {
        self.get_balance(address, crate::config::SOL_CHAIN_ID)
            .await
            .unwrap()
            .and_then(|a| a.solana_address())
    }

    fn is_valid_chain_id(&self, chain_id: u64) -> bool {
        self.chains().any(|c| c.id == chain_id)
    }

    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey {
        self.chains().find(|c| c.id == chain_id).unwrap().token
    }

    fn default_chain_id(&self) -> u64 {
        self.default_chain()
    }

    async fn contract_chain_id(&self, address: Address) -> Result<u64> {
        let contract = self.get_contract(address).await?;
        let chain_id = contract.map_or_else(|| self.default_chain(), |c| c.chain_id());
        Ok(chain_id)
    }

    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8) {
        pda::contract_address(&self.program_id(), &address)
    }

    fn balance_pubkey(&self, address: Address, chain_id: u64) -> (Pubkey, u8) {
        pda::balance_address(&self.program_id(), &address, chain_id)
    }

    fn storage_cell_pubkey(&self, address: Address, index: U256) -> Pubkey {
        if index < U256::from(crate::config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
            self.contract_pubkey(address).0
        } else {
            let index = index & !U256::new(0xFF);

            let base = self.contract_pubkey(address).0;
            let seed = StorageCellSeed::new(index);
            Pubkey::create_with_seed(&base, &seed, &self.program_id()).unwrap()
        }
    }

    async fn code_size(&self, address: Address) -> usize {
        self.get_contract(address)
            .await
            .unwrap()
            .map_or(0, |a| a.code_len())
    }

    async fn code(&self, address: Address) -> Vector<u8> {
        self.get_contract(address)
            .await
            .unwrap()
            .map_or_else(|| vector![], |a| a.code().to_vector())
    }

    async fn storage(&self, address: Address, index: U256) -> [u8; 32] {
        if index < U256::from(crate::config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
            let index: usize = index.as_usize();
            return self
                .get_contract(address)
                .await
                .unwrap()
                .map(|c| c.storage_value(index))
                .unwrap_or_default();
        }

        let subindex = (index & 0xFF).as_u8();
        let index = index & !U256::new(0xFF);

        self.get_storage(address, index)
            .await
            .unwrap()
            .map(|a| a.get(subindex))
            .unwrap_or_default()
    }

    async fn clone_solana_account(&self, address: &Pubkey) -> OwnedAccountInfo {
        let account = if address == &FAKE_OPERATOR {
            let operator = self.operator();
            self.get_account(operator).await.unwrap()
        } else {
            self.get_account(*address).await.unwrap()
        };

        OwnedAccountInfo::from_account(self.program_id(), &account)
    }

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&Account) -> R,
    {
        let info = self.get_account(*address).await.unwrap();
        action(&info)
    }
}

#[maybe_async(?Send)]
impl<'a, T: Platform<'a>> SyncedAccountStorage for T {
    async fn set_code(&mut self, address: Address, chain_id: u64, code: Vector<u8>) -> Result<()> {
        let mut contract = self.create_contract(address, chain_id).await?;
        contract.allocate_entire_code_buffer(&code)?;
        contract.set_code(&code)
    }

    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        if index < U256::from(crate::config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
            let mut contract = self.get_contract(address).await?.unwrap();
            contract.set_storage_value(index.as_usize(), &value)?;
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            if value == [0; 32] {
                if let Some(mut storage) = self.get_storage(address, index).await? {
                    storage.update(subindex, &value)?;
                }
            } else {
                let mut storage = self.create_storage(address, index).await?;
                storage.update(subindex, &value)?;
            }
        }

        Ok(())
    }

    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let mut balance = self.create_balance(address, chain_id).await?;
        balance.increment_nonce()
    }

    async fn transfer(
        &mut self,
        from_address: Address,
        to_address: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        let mut source = self.create_balance(from_address, chain_id).await?;
        let mut target = self.create_balance(to_address, chain_id).await?;
        source.transfer(&mut target, value)
    }

    async fn burn(&mut self, address: Address, chain_id: u64, value: U256) -> Result<()> {
        let mut balance = self.create_balance(address, chain_id).await?;
        balance.burn(value)
    }

    async fn execute_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: &[&[&[u8]]],
        emulated_internally: bool,
    ) -> Result<()> {
        let mode = if emulated_internally {
            crate::platform::InvokeMode::Queued
        } else {
            crate::platform::InvokeMode::Normal
        };
        self.invoke(instruction, seeds, mode).await
    }

    fn snapshot(&mut self) {
        self.snapshot();
    }

    fn revert_snapshot(&mut self) {
        self.revert();
    }

    fn commit_snapshot(&mut self) {
        self.commit();
    }
}
