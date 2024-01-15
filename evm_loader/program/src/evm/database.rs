use super::{Buffer, Context};
use crate::{error::Result, types::Address};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

#[maybe_async(?Send)]
pub trait Database {
    fn default_chain_id(&self) -> u64;
    fn is_valid_chain_id(&self, chain_id: u64) -> bool;
    async fn contract_chain_id(&self, address: Address) -> Result<u64>;

    async fn nonce(&self, address: Address, chain_id: u64) -> Result<u64>;
    fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()>;

    async fn balance(&self, address: Address, chain_id: u64) -> Result<U256>;
    async fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()>;
    async fn code_size(&self, address: Address) -> Result<usize>;
    async fn code(&self, address: Address) -> Result<Buffer>;
    fn set_code(&mut self, address: Address, chain_id: u64, code: Vec<u8>) -> Result<()>;
    fn selfdestruct(&mut self, address: Address) -> Result<()>;

    async fn storage(&self, address: Address, index: U256) -> Result<[u8; 32]>;
    fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()>;

    async fn block_hash(&self, number: U256) -> Result<[u8; 32]>;
    fn block_number(&self) -> Result<U256>;
    fn block_timestamp(&self) -> Result<U256>;

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&AccountInfo) -> R;

    fn snapshot(&mut self);
    fn revert_snapshot(&mut self);
    fn commit_snapshot(&mut self);

    async fn precompile_extension(
        &mut self,
        context: &Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>>;
}

/// Provides convenience methods that can be implemented in terms of `Database`.
#[maybe_async(?Send)]
pub trait DatabaseExt {
    /// Returns whether an account exists, which is also referred to as the account being non-empty,
    /// in accordance with https://eips.ethereum.org/EIPS/eip-161.
    async fn account_exists(&self, address: Address, chain_id: u64) -> Result<bool>;

    /// Returns the code has for an address in accordance with https://eips.ethereum.org/EIPS/eip-1052.
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
        let bytes_to_hash = if !code.is_empty() {
            Some(&*code)
        } else if self.account_exists(address, chain_id).await? {
            Some(<&[u8]>::default())
        } else {
            None
        };

        Ok(bytes_to_hash
            .map(|bytes| solana_program::keccak::hash(bytes).to_bytes())
            .unwrap_or_default())
    }
}
