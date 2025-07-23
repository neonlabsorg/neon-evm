use crate::{
    error::Result,
    types::{vector::VectorSliceExt, Address, Vector},
};
use allocator_api2::alloc::Allocator;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::keccak;

use super::Context;

#[maybe_async(?Send)]
pub trait Database {
    fn default_chain_id(&self) -> u64;
    async fn contract_chain_id(&self, address: Address) -> Result<u64>;
    async fn log_event<const N: usize>(
        &mut self,
        address: Address,
        topics: [[u8; 32]; N],
        data: &[u8],
    ) -> Result<()>;

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

    async fn code_size(&self, address: Address) -> Result<usize>;
    async fn use_code<R, F>(&self, address: Address, action: F) -> Result<R>
    where
        F: for<'a> FnOnce(&'a [u8]) -> R;

    async fn start_create(&mut self, address: Address, chain_id: u64) -> Result<()>;
    async fn end_create(&mut self, address: Address, code: &[u8]) -> Result<()>;

    async fn storage(&self, address: Address, index: U256) -> Result<[u8; 32]>;
    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()>;

    async fn transient_storage(&self, address: Address, index: U256) -> Result<[u8; 32]>;
    fn set_transient_storage(
        &mut self,
        address: Address,
        index: U256,
        value: [u8; 32],
    ) -> Result<()>;

    async fn block_hash(&self, number: U256, context: &Context) -> Result<[u8; 32]>;
    async fn block_number(&self, context: &Context) -> Result<U256>;
    async fn block_timestamp(&self, context: &Context) -> Result<U256>;

    async fn precompile_extension(
        &mut self,
        context: &Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>>;

    fn snapshot(&mut self);
    fn revert_snapshot(&mut self);
    fn commit_snapshot(&mut self);

    async fn account_exists(&self, address: Address, chain_id: u64) -> Result<bool> {
        Ok(self.nonce(address, chain_id).await? > 0 || self.balance(address, chain_id).await? > 0)
    }

    async fn code_hash(&self, address: Address, chain_id: u64) -> Result<keccak::Hash> {
        if !self.account_exists(address, chain_id).await? {
            return Ok(keccak::Hash::default());
        }

        self.use_code(address, keccak::hash).await
    }

    #[inline]
    async fn code<A: Allocator>(&self, address: Address, allocator: A) -> Result<Vector<u8, A>> {
        self.use_code(address, |c| c.to_vector(allocator)).await
    }
}
