use crate::{
    account_storage::FAKE_OPERATOR,
    error::Result,
    evm::{database::Database, Context},
    types::Address,
};
use maybe_async::maybe_async;
use solana_program::{pubkey::Pubkey, system_instruction};

use super::OwnedAccountInfo;

mod call_solana;
mod metaplex;
mod neon_token;
mod query_account;
mod spl_token;

pub struct PrecompiledContracts {}

impl PrecompiledContracts {
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

    #[must_use]
    pub fn is_precompile_extension(address: &Address) -> bool {
        *address == Self::SYSTEM_ACCOUNT_QUERY
            || *address == Self::SYSTEM_ACCOUNT_NEON_TOKEN
            || *address == Self::SYSTEM_ACCOUNT_SPL_TOKEN
            || *address == Self::SYSTEM_ACCOUNT_METAPLEX
            || *address == Self::SYSTEM_ACCOUNT_CALL_SOLANA
    }

    #[maybe_async]
    pub async fn call_precompile_extension<State: Database>(
        state: &mut State,
        context: &Context,
        address: &Address,
        input: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>> {
        match *address {
            Self::SYSTEM_ACCOUNT_QUERY => {
                Some(query_account::query_account(state, address, input, context, is_static).await)
            }
            Self::SYSTEM_ACCOUNT_NEON_TOKEN => {
                Some(neon_token::neon_token(state, address, input, context, is_static).await)
            }
            Self::SYSTEM_ACCOUNT_SPL_TOKEN => {
                Some(spl_token::spl_token(state, address, input, context, is_static).await)
            }
            Self::SYSTEM_ACCOUNT_METAPLEX => {
                Some(metaplex::metaplex(state, address, input, context, is_static).await)
            }
            Self::SYSTEM_ACCOUNT_CALL_SOLANA => {
                Some(call_solana::call_solana(state, address, input, context, is_static).await)
            }
            _ => None,
        }
    }
}

#[maybe_async]
pub async fn create_account<State: Database>(
    state: &mut State,
    account: &OwnedAccountInfo,
    space: usize,
    owner: &Pubkey,
    seeds: Vec<Vec<u8>>,
) -> Result<()> {
    let minimum_balance = state.rent().minimum_balance(space);

    let required_lamports = minimum_balance.saturating_sub(account.lamports);

    if required_lamports > 0 {
        let transfer =
            system_instruction::transfer(&FAKE_OPERATOR, &account.key, required_lamports);
        state
            .queue_external_instruction(transfer, vec![], required_lamports, true)
            .await?;
    }

    let allocate = system_instruction::allocate(&account.key, space.try_into().unwrap());
    state
        .queue_external_instruction(allocate, vec![seeds.clone()], 0, true)
        .await?;

    let assign = system_instruction::assign(&account.key, owner);
    state
        .queue_external_instruction(assign, vec![seeds], 0, true)
        .await?;

    Ok(())
}
