use super::{Buffer, Context};
use crate::account_storage::LogCollector;
use crate::types::Vector;
use crate::{error::Result, executor::OwnedAccountInfo, types::Address};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{
    account_info::AccountInfo, instruction::Instruction, pubkey::Pubkey, rent::Rent,
};

#[maybe_async(?Send)]
pub trait Database: LogCollector {
    fn program_id(&self) -> &Pubkey;
    fn operator(&self) -> Pubkey;
    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey;
    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8);

    fn default_chain_id(&self) -> u64;
    fn is_valid_chain_id(&self, chain_id: u64) -> bool;
    async fn contract_chain_id(&self, address: Address) -> Result<u64>;

    async fn nonce(&self, address: Address, chain_id: u64) -> Result<u64>;
    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()>;

    async fn balance(&self, address: Address, chain_id: u64) -> Result<U256>;
    async fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()>;
    async fn burn(&mut self, address: Address, chain_id: u64, value: U256) -> Result<()>;

    async fn code_size(&self, address: Address) -> Result<usize>;
    async fn code(&self, address: Address) -> Result<Buffer>;
    async fn set_code(&mut self, address: Address, chain_id: u64, code: Vector<u8>) -> Result<()>;

    async fn storage(&self, address: Address, index: U256) -> Result<[u8; 32]>;
    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()>;

    async fn transient_storage(&self, address: Address, index: U256) -> Result<[u8; 32]>;
    fn set_transient_storage(
        &mut self,
        address: Address,
        index: U256,
        value: [u8; 32],
    ) -> Result<()>;

    async fn block_hash(&self, number: U256) -> Result<[u8; 32]>;
    fn block_number(&self) -> Result<U256>;
    fn block_timestamp(&self) -> Result<U256>;
    fn rent(&self) -> &Rent;
    fn return_data(&self) -> Option<(Pubkey, Vector<u8>)>;
    fn set_return_data(&mut self, data: &[u8]);

    async fn external_account(&self, address: Pubkey) -> Result<OwnedAccountInfo>;
    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&AccountInfo) -> R;

    fn snapshot(&mut self);
    fn revert_snapshot(&mut self);
    fn commit_snapshot(&mut self);

    async fn queue_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: Vector<Vector<Vector<u8>>>,
        fee: u64,
        emulated_internally: bool,
    ) -> Result<()>;

    async fn precompile_extension(
        &mut self,
        context: &Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vector<u8>>>;
}

/// Provides convenience methods that can be implemented in terms of `Database`.
#[maybe_async(?Send)]
pub trait DatabaseExt {
    /// Returns whether an account exists and is non-empty as specified in
    /// https://eips.ethereum.org/EIPS/eip-161.
    async fn account_exists(&self, address: Address, chain_id: u64) -> Result<bool>;

    /// Returns the code hash for an address as specified in
    /// https://eips.ethereum.org/EIPS/eip-1052.
    async fn code_hash(&self, address: Address, chain_id: u64) -> Result<[u8; 32]>;
}

#[maybe_async(?Send)]
impl<T: Database> DatabaseExt for T {
    async fn account_exists(&self, address: Address, chain_id: u64) -> Result<bool> {
        Ok(self.nonce(address, chain_id).await? > 0 || self.balance(address, chain_id).await? > 0)
    }

    async fn code_hash(&self, address: Address, chain_id: u64) -> Result<[u8; 32]> {
        // The function `Database::code` returns a zero-length buffer if the account exists with
        // zero-length code, but also when the account does not exist. This makes it necessary to
        // also check if the account exists when the returned buffer is empty.
        //
        // We could simplify the implementation by checking if the account exists first, but that
        // would lead to more computation in what we think is the common case where the account
        // exists and contains code.
        let code = self.code(address).await?;
        let bytes_to_hash: Option<&[u8]> = if !code.is_empty() {
            Some(&*code)
        } else if self.account_exists(address, chain_id).await? {
            Some(&[])
        } else {
            None
        };

        Ok(bytes_to_hash.map_or([0; 32], |bytes| {
            solana_program::keccak::hash(bytes).to_bytes()
        }))
    }
}
