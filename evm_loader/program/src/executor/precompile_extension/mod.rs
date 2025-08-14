use crate::account::{Account, AccountDispatch};
use crate::platform::{Chain, KeysIndex};
use crate::{
    error::Result,
    evm::{database::Database, Context},
    platform::FAKE_OPERATOR,
    types::Address,
};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_system_interface::instruction as system_instruction;

use super::owned_account::OwnedAccountInfo;

pub mod call_solana;
mod metaplex;
mod neon_account;
mod neon_token;
mod query_account;
mod spl_token;

#[maybe_async(?Send)]
pub trait PrecompileDatabase: Database {
    fn is_iterative_mode(&self) -> bool;

    fn program_id(&self) -> Pubkey;
    fn operator(&self) -> Pubkey;
    fn chains(&self) -> impl Iterator<Item = Chain>;
    async fn rent(&self) -> Result<Rent>;

    fn keys(&self) -> &impl KeysIndex;

    async fn invoke(&mut self, instruction: Instruction, seeds: &[&[&[u8]]]) -> Result<()>;
    async fn queue_invoke(&mut self, instruction: Instruction, seeds: &[&[&[u8]]]) -> Result<()>;

    async fn external_account(&self, pubkey: Pubkey) -> Result<OwnedAccountInfo>;
    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)>;

    async fn solana_user_pubkey(&self, address: Address) -> Result<Option<Pubkey>>;
    async fn burn(&mut self, address: Address, chain_id: u64, amount: U256) -> Result<()>;

    async fn use_real_account<R, F>(&self, pubkey: Pubkey, f: F) -> Result<R>
    where
        F: FnOnce(&Account) -> R;

    async fn real_lamports(&self, pubkey: Pubkey) -> Result<u64> {
        self.use_real_account(pubkey, |a| a.lamports()).await
    }
}

#[deprecated]
const _SYSTEM_ACCOUNT_ERC20_WRAPPER: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
]);
const SYSTEM_ACCOUNT_QUERY: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02,
]);
const SYSTEM_ACCOUNT_NEON_TOKEN: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x03,
]);
const SYSTEM_ACCOUNT_SPL_TOKEN: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x04,
]);
const SYSTEM_ACCOUNT_METAPLEX: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x05,
]);
const SYSTEM_ACCOUNT_CALL_SOLANA: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x06,
]);
const SYSTEM_ACCOUNT_NEON_ACCOUNT: Address = Address([
    0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x07,
]);

#[must_use]
pub fn is_precompile_extension(address: &Address) -> bool {
    *address == SYSTEM_ACCOUNT_QUERY
        || *address == SYSTEM_ACCOUNT_NEON_TOKEN
        || *address == SYSTEM_ACCOUNT_SPL_TOKEN
        || *address == SYSTEM_ACCOUNT_METAPLEX
        || *address == SYSTEM_ACCOUNT_CALL_SOLANA
        || *address == SYSTEM_ACCOUNT_NEON_ACCOUNT
}

#[maybe_async]
pub async fn call_precompile_extension(
    state: &mut impl PrecompileDatabase,
    context: &Context,
    address: &Address,
    input: &[u8],
    is_static: bool,
) -> Option<Result<Vec<u8>>> {
    match *address {
        SYSTEM_ACCOUNT_QUERY => {
            Some(query_account::query_account(state, address, input, context, is_static).await)
        }
        SYSTEM_ACCOUNT_NEON_TOKEN => {
            Some(neon_token::neon_token(state, address, input, context, is_static).await)
        }
        SYSTEM_ACCOUNT_SPL_TOKEN => {
            Some(spl_token::spl_token(state, address, input, context, is_static).await)
        }
        SYSTEM_ACCOUNT_METAPLEX => {
            Some(metaplex::metaplex(state, address, input, context, is_static).await)
        }
        SYSTEM_ACCOUNT_CALL_SOLANA => {
            Some(call_solana::call_solana(state, address, input, context, is_static).await)
        }
        SYSTEM_ACCOUNT_NEON_ACCOUNT => {
            Some(neon_account::neon_account(state, address, input, context, is_static).await)
        }
        _ => None,
    }
}

#[maybe_async]
pub async fn create_account(
    state: &mut impl PrecompileDatabase,
    account: &OwnedAccountInfo,
    space: usize,
    owner: &Pubkey,
    seeds: &[&[u8]],
) -> Result<()> {
    let rent = state.rent().await?;
    let minimum_balance = rent.minimum_balance(space);

    let lamports = minimum_balance.saturating_sub(account.lamports);
    if lamports > 0 {
        let transfer = system_instruction::transfer(&FAKE_OPERATOR, &account.key, lamports);
        state.queue_invoke(transfer, &[]).await?;
    }

    let allocate = system_instruction::allocate(&account.key, space.try_into()?);
    state.queue_invoke(allocate, &[seeds]).await?;

    let assign = system_instruction::assign(&account.key, owner);
    state.queue_invoke(assign, &[seeds]).await?;

    Ok(())
}
