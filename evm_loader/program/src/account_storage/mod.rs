use crate::account::Account;
use crate::executor::OwnedAccountInfo;
use crate::types::Address;
use crate::{error::Result, types::Vector};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{instruction::Instruction, pubkey::Pubkey, rent::Rent};

mod block_hash;

mod platform_backend;

/// Account storage
/// Trait to access account info
#[maybe_async(?Send)]
pub trait AccountStorage: LogCollector {
    /// Get `NeonEVM` program id
    fn program_id(&self) -> Pubkey;
    /// Get operator pubkey
    fn operator(&self) -> Pubkey;

    /// Get block number
    async fn block_number(&self) -> U256;
    /// Get block timestamp
    async fn block_timestamp(&self) -> U256;
    /// Get block hash
    async fn block_hash(&self, number: u64) -> [u8; 32];

    /// Get rent info
    async fn rent(&self) -> Rent;

    /// Get return data from Solana
    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)>;

    /// Set return data to Solana
    fn set_return_data(&mut self, data: &[u8]);

    /// Get account nonce
    async fn nonce(&self, address: Address, chain_id: u64) -> u64;
    /// Get account balance
    async fn balance(&self, address: Address, chain_id: u64) -> U256;
    /// Get solana user pubkey
    async fn solana_user_address(&self, address: Address) -> Option<Pubkey>;

    fn is_valid_chain_id(&self, chain_id: u64) -> bool;
    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey;
    fn default_chain_id(&self) -> u64;

    /// Get contract chain_id
    async fn contract_chain_id(&self, address: Address) -> Result<u64>;

    /// Get contract solana address
    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8);
    /// Get balance solana address
    fn balance_pubkey(&self, address: Address, chain_id: u64) -> (Pubkey, u8);
    /// Get cell solana address
    fn storage_cell_pubkey(&self, address: Address, index: U256) -> Pubkey;

    /// Get code size
    async fn code_size(&self, address: Address) -> usize;
    /// Get code data
    async fn code(&self, address: Address) -> Vector<u8>;

    /// Get data from storage
    async fn storage(&self, address: Address, index: U256) -> [u8; 32];

    /// Clone existing solana account
    async fn clone_solana_account(&self, address: &Pubkey) -> OwnedAccountInfo;

    /// Map existing solana account
    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&Account) -> R;
}

#[maybe_async(?Send)]
pub trait SyncedAccountStorage: AccountStorage {
    async fn start_create(&mut self, address: Address, chain_id: u64) -> Result<()>;
    async fn end_create(&mut self, address: Address, code: &[u8]) -> Result<()>;

    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()>;
    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()>;
    async fn transfer(
        &mut self,
        from_address: Address,
        to_address: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()>;
    async fn burn(&mut self, address: Address, chain_id: u64, value: U256) -> Result<()>;
    async fn execute_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: &[&[&[u8]]],
        emulated_internally: bool,
    ) -> Result<()>;

    fn snapshot(&mut self);
    fn revert_snapshot(&mut self);
    fn commit_snapshot(&mut self);
}

#[maybe_async(?Send)]
pub trait LogCollector {
    async fn collect_log<const N: usize>(
        &mut self,
        address: &[u8; 20],
        topics: [[u8; 32]; N],
        data: &[u8],
    );
}
