use ethnum::U256;
use maybe_async::maybe_async;

use evm_loader::account_storage::AccountStorage;
use evm_loader::error::Result;
use evm_loader::evm::database::Database;
use evm_loader::evm::Buffer as EvmBuffer;
use evm_loader::evm::{Context, ExitStatus};
use evm_loader::executor::{
    precompile_extension::PrecompiledContracts, Action, ExecutorState, OwnedAccountInfo,
};
use evm_loader::types::Address;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::rent::Rent;

#[derive(Default, Clone, Copy)]
pub struct ExecuteStatus {
    pub external_solana_calls: bool,
    pub reverts_before_solana_calls: bool,
    pub reverts_after_solana_calls: bool,
}

/// Represents the state of executor abstracted away from a self.backend.
/// UPDATE `serialize/deserialize` WHEN THIS STRUCTURE CHANGES
pub struct EmulatorState<'a, B: AccountStorage> {
    inner_state: ExecutorState<'a, B>,
    pub execute_status: ExecuteStatus,
}

impl<'a, B: AccountStorage> EmulatorState<'a, B> {
    #[must_use]
    pub fn new(backend: &'a B) -> Self {
        Self {
            inner_state: ExecutorState::new(backend),
            execute_status: ExecuteStatus::default(),
        }
    }

    pub fn into_actions(self) -> Vec<Action> {
        self.inner_state.into_actions()
    }

    pub fn exit_status(&self) -> Option<&ExitStatus> {
        self.inner_state.exit_status()
    }

    pub fn set_exit_status(&mut self, status: ExitStatus) {
        self.inner_state.set_exit_status(status);
    }

    pub fn call_depth(&self) -> usize {
        self.inner_state.call_depth()
    }
}

#[maybe_async(?Send)]
impl<'a, B: AccountStorage> Database for EmulatorState<'a, B> {
    fn program_id(&self) -> &Pubkey {
        self.inner_state.program_id()
    }
    fn operator(&self) -> Pubkey {
        self.inner_state.operator()
    }
    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey {
        self.inner_state.chain_id_to_token(chain_id)
    }
    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8) {
        self.inner_state.contract_pubkey(address)
    }

    async fn nonce(&self, from_address: Address, from_chain_id: u64) -> Result<u64> {
        self.inner_state.nonce(from_address, from_chain_id).await
    }

    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        self.inner_state.increment_nonce(address, chain_id).await
    }

    async fn balance(&self, from_address: Address, from_chain_id: u64) -> Result<U256> {
        self.inner_state.balance(from_address, from_chain_id).await
    }

    async fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        self.inner_state
            .transfer(source, target, chain_id, value)
            .await
    }

    async fn burn(&mut self, source: Address, chain_id: u64, value: U256) -> Result<()> {
        self.inner_state.burn(source, chain_id, value).await
    }

    async fn code_size(&self, from_address: Address) -> Result<usize> {
        self.inner_state.code_size(from_address).await
    }

    async fn code(&self, from_address: Address) -> Result<EvmBuffer> {
        self.inner_state.code(from_address).await
    }

    async fn set_code(&mut self, address: Address, chain_id: u64, code: Vec<u8>) -> Result<()> {
        self.inner_state.set_code(address, chain_id, code).await
    }

    fn selfdestruct(&mut self, address: Address) -> Result<()> {
        self.inner_state.selfdestruct(address)
    }

    async fn storage(&self, from_address: Address, from_index: U256) -> Result<[u8; 32]> {
        self.inner_state.storage(from_address, from_index).await
    }

    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        self.inner_state.set_storage(address, index, value).await
    }

    async fn block_hash(&self, number: U256) -> Result<[u8; 32]> {
        self.inner_state.block_hash(number).await
    }

    fn block_number(&self) -> Result<U256> {
        self.inner_state.block_number()
    }

    fn block_timestamp(&self) -> Result<U256> {
        self.inner_state.block_timestamp()
    }

    async fn external_account(&self, address: Pubkey) -> Result<OwnedAccountInfo> {
        self.inner_state.external_account(address).await
    }

    fn rent(&self) -> &Rent {
        self.inner_state.rent()
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.inner_state.return_data()
    }

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&solana_sdk::account_info::AccountInfo) -> R,
    {
        self.inner_state.map_solana_account(address, action).await
    }

    fn snapshot(&mut self) {
        self.inner_state.snapshot()
    }

    fn revert_snapshot(&mut self) {
        if self.execute_status.external_solana_calls {
            self.execute_status.reverts_after_solana_calls = true;
        } else {
            self.execute_status.reverts_before_solana_calls = true;
        }

        self.inner_state.revert_snapshot()
    }

    fn commit_snapshot(&mut self) {
        self.inner_state.commit_snapshot()
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
        self.inner_state.default_chain_id()
    }

    fn is_valid_chain_id(&self, chain_id: u64) -> bool {
        self.inner_state.is_valid_chain_id(chain_id)
    }

    async fn contract_chain_id(&self, contract: Address) -> Result<u64> {
        self.inner_state.contract_chain_id(contract).await
    }

    async fn queue_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: Vec<Vec<Vec<u8>>>,
        fee: u64,
        emulated_internally: bool,
    ) -> Result<()> {
        if !emulated_internally {
            self.execute_status.external_solana_calls = true;
        }

        self.inner_state
            .queue_external_instruction(instruction, seeds, fee, emulated_internally).await
    }
}
