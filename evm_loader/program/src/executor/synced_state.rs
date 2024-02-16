use ethnum::{AsU256, U256};
use maybe_async::maybe_async;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;

use crate::account_storage::{AccountStorage, SyncedAccountStorage};
use crate::error::{Error, Result};
use crate::evm::database::Database;
use crate::evm::Context;
use crate::types::Address;

use super::precompile_extension::PrecompiledContracts;
use super::OwnedAccountInfo;

pub struct SyncedExecutorState<'a, B: AccountStorage> {
    pub backend: &'a mut B,
    depth: usize,
}

impl<'a, B: AccountStorage + SyncedAccountStorage> SyncedExecutorState<'a, B> {
    #[must_use]
    pub fn new(backend: &'a mut B) -> Self {
        Self { backend, depth: 0 }
    }
}

#[maybe_async(?Send)]
impl<'a, B: AccountStorage + SyncedAccountStorage> Database for SyncedExecutorState<'a, B> {
    fn program_id(&self) -> &Pubkey {
        self.backend.program_id()
    }
    fn operator(&self) -> Pubkey {
        self.backend.operator()
    }
    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey {
        self.backend.chain_id_to_token(chain_id)
    }
    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8) {
        self.backend.contract_pubkey(address)
    }

    async fn nonce(&self, from_address: Address, from_chain_id: u64) -> Result<u64> {
        let nonce = self.backend.nonce(from_address, from_chain_id).await;
        Ok(nonce)
    }

    fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        self.backend.increment_nonce(address, chain_id)?;
        Ok(())
    }

    async fn balance(&self, from_address: Address, from_chain_id: u64) -> Result<U256> {
        let balance = self.backend.balance(from_address, from_chain_id).await;
        Ok(balance)
    }

    async fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        if value == U256::ZERO {
            return Ok(());
        }

        let target_chain_id = self.contract_chain_id(target).await.unwrap_or(chain_id);

        if (self.code_size(target).await? > 0) && (target_chain_id != chain_id) {
            return Err(Error::InvalidTransferToken(source, chain_id));
        }

        if source == target {
            return Ok(());
        }

        if self.balance(source, chain_id).await? < value {
            return Err(Error::InsufficientBalance(source, chain_id, value));
        }

        self.backend.transfer(source, target, chain_id, value)?;
        Ok(())
    }

    async fn burn(&mut self, source: Address, chain_id: u64, value: U256) -> Result<()> {
        self.backend.burn(source, chain_id, value)?;
        Ok(())
    }

    async fn code_size(&self, from_address: Address) -> Result<usize> {
        if PrecompiledContracts::is_precompile_extension(&from_address) {
            return Ok(1); // This is required in order to make a normal call to an extension contract
        }

        Ok(self.backend.code_size(from_address).await)
    }

    async fn code(&self, from_address: Address) -> Result<crate::evm::Buffer> {
        Ok(self.backend.code(from_address).await)
    }

    fn set_code(&mut self, address: Address, chain_id: u64, code: Vec<u8>) -> Result<()> {
        if code.starts_with(&[0xEF]) {
            // https://eips.ethereum.org/EIPS/eip-3541
            return Err(Error::EVMObjectFormatNotSupported(address));
        }

        if code.len() > 0x6000 {
            // https://eips.ethereum.org/EIPS/eip-170
            return Err(Error::ContractCodeSizeLimit(address, code.len()));
        }

        self.backend.set_code(address, chain_id, code)?;
        Ok(())
    }

    fn selfdestruct(&mut self, _address: Address) -> Result<()> {
        Err(Error::Custom("Selfdestruct is not supported".to_string()))
    }

    async fn storage(&self, from_address: Address, from_index: U256) -> Result<[u8; 32]> {
        Ok(self.backend.storage(from_address, from_index).await)
    }

    fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        self.backend.set_storage(address, index, value)?;
        Ok(())
    }

    async fn block_hash(&self, number: U256) -> Result<[u8; 32]> {
        // geth:
        //  - checks the overflow
        //  - converts to u64
        //  - checks on last 256 blocks

        if number >= u64::MAX.as_u256() {
            return Ok(<[u8; 32]>::default());
        }

        let number = number.as_u64();
        let block_slot = self.backend.block_number().as_u64();
        let lower_block_slot = if block_slot < 257 {
            0
        } else {
            block_slot - 256
        };

        if number >= block_slot || lower_block_slot > number {
            return Ok(<[u8; 32]>::default());
        }

        Ok(self.backend.block_hash(number).await)
    }

    fn block_number(&self) -> Result<U256> {
        Ok(self.backend.block_number())
    }

    fn block_timestamp(&self) -> Result<U256> {
        Ok(self.backend.block_timestamp())
    }

    fn rent(&self) -> &Rent {
        self.backend.rent()
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.backend.return_data()
    }

    async fn external_account(&self, address: Pubkey) -> Result<OwnedAccountInfo> {
        let account = self.backend.clone_solana_account(&address).await;
        return Ok(account);
    }

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&solana_program::account_info::AccountInfo) -> R,
    {
        self.backend.map_solana_account(address, action).await
    }

    fn snapshot(&mut self) {
        self.depth += 1;
    }

    fn revert_snapshot(&mut self) {
        panic!("revert snapshot not implemented for SyncedExecutorState");
    }

    fn commit_snapshot(&mut self) {
        self.depth
            .checked_sub(1)
            .expect("Fatal Error: Inconsistent EVM Call Stack");
    }

    async fn precompile_extension(
        &mut self,
        context: &Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>> {
        PrecompiledContracts::call_precompile_extension(self, context, address, data, is_static)
            .await
    }

    fn default_chain_id(&self) -> u64 {
        self.backend.default_chain_id()
    }

    fn is_valid_chain_id(&self, chain_id: u64) -> bool {
        self.backend.is_valid_chain_id(chain_id)
    }

    async fn contract_chain_id(&self, contract: Address) -> Result<u64> {
        self.backend.contract_chain_id(contract).await
    }

    fn queue_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: Vec<Vec<Vec<u8>>>,
        fee: u64,
        _emulated_internally: bool,
    ) -> Result<()> {
        self.backend
            .execute_external_instruction(instruction, seeds, fee)?;
        Ok(())
    }
}
