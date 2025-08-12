#![allow(warnings)]
use std::convert::TryInto;

use arrayref::{array_ref, array_refs};
use ethnum::U256;
use maybe_async::maybe_async;
use pinocchio_token_interface::state;
use solana_program::{account_info::IntoAccountInfo, pubkey::Pubkey};

use crate::account::pda;
use crate::executor::external_programs::spl_associated_token::{
    get_associated_token_address, CreateAssociatedToken,
};
use crate::executor::external_programs::spl_token::Transfer;
use crate::executor::external_programs::{SplAssociatedToken, SplToken};
use crate::platform::Platform;

use crate::types::seeds::Seeds;
use crate::{
    account::token,
    error::{Error, Result},
    platform::FAKE_OPERATOR,
    types::Address,
};

use super::PrecompileDatabase;

// Neon token method ids:
//--------------------------------------------------
// withdraw(bytes32)           => 8e19899e
// withdraw_on_chain(uint256,bytes32,uint256) => 78db6706
//--------------------------------------------------
const NEON_TOKEN_METHOD_WITHDRAW_ID: &[u8; 4] = &[0x8e, 0x19, 0x89, 0x9e];
const NEON_TOKEN_METHOD_WITHDRAW_ON_CHAIN_ID: &[u8; 4] = &[0x78, 0xdb, 0x67, 0x06];

#[maybe_async]
pub async fn neon_token(
    state: &mut impl PrecompileDatabase,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    is_static: bool,
) -> Result<Vec<u8>> {
    debug_print!("neon_token({})", hex::encode(input));

    if &context.contract != address {
        return Err(Error::Custom(
            "Withdraw: callcode or delegatecall is not allowed".to_string(),
        ));
    }

    let (method_id, rest) = input.split_at(4);
    let method_id: &[u8; 4] = method_id.try_into().unwrap_or(&[0_u8; 4]);

    if method_id == NEON_TOKEN_METHOD_WITHDRAW_ID {
        if is_static {
            return Err(Error::StaticModeViolation(*address));
        }

        let source = context.contract;
        let chain_id = context.contract_chain_id;
        let value = context.value;
        // owner of the associated token account
        let destination = array_ref![rest, 0, 32];
        let destination = Pubkey::new_from_array(*destination);

        withdraw(state, source, chain_id, destination, value).await?;

        let mut output = vec![0_u8; 32];
        output[31] = 1; // return true

        return Ok(output);
    };

    if method_id == NEON_TOKEN_METHOD_WITHDRAW_ON_CHAIN_ID {
        if is_static {
            return Err(Error::StaticModeViolation(*address));
        }
        // withdraw_on_chain(uint256 chainId, bytes32 to, uint256 amount)
        let (chain_id, dest, amount) = array_refs![rest.try_into()?, 32, 32, 32];
        let chain_id = U256::from_be_bytes(*chain_id).try_into()?;
        if context.value != 0 {
            return Err(Error::InvalidTransferToken(*address, chain_id));
        }
        let dest = Pubkey::new_from_array(*dest);
        let amount = U256::from_be_bytes(*amount);

        withdraw(state, context.caller, chain_id, dest, amount).await?;

        let mut output = vec![0_u8; 32];
        output[31] = 1; // return true

        return Ok(output);
    };

    debug_print!("neon_token UNKNOWN");
    Err(Error::UnknownPrecompileMethodSelector(*address, *method_id))
}

#[maybe_async]
async fn withdraw(
    state: &mut impl PrecompileDatabase,
    source: Address,
    chain_id: u64,
    target: Pubkey,
    value: U256,
) -> Result<()> {
    if value == 0 {
        return Err(Error::Custom("Neon Withdraw: value == 0".to_string()));
    }

    let mint_address = {
        let Some(mint_chain) = state.chains().find(|c| c.id == chain_id) else {
            return Err(Error::InvalidChainId(chain_id));
        };
        mint_chain.token
    };

    let decimals = {
        let mint_account = state.external_account(&mint_address).await?;
        let mint = unsafe { state::load::<state::mint::Mint>(&mint_account.data) }?;
        mint.decimals
    };

    assert!(decimals < 18);

    let additional_decimals: u32 = (18 - decimals).into();
    let min_amount: u128 = u128::pow(10, additional_decimals);

    let spl_amount = value / min_amount;
    let remainder = value % min_amount;

    if spl_amount > U256::from(u64::MAX) {
        return Err("Neon Withdraw: value exceeds u64::max".into());
    }

    if remainder != 0 {
        return Err(Error::Custom(std::format!(
            "Neon Withdraw: value must be divisible by 10^{additional_decimals}"
        )));
    }

    let target_token = get_associated_token_address(&target, &mint_address);

    let create_associated = SplAssociatedToken::CreateIdempotent(CreateAssociatedToken {
        account: target_token,
        owner: target,
        mint: mint_address,
    });
    state.queue_invoke(create_associated).await?;

    let (authority, bump_seed) = pda::main_pool_authority(state.program_id());
    let authority_seeds: &[&[u8]] = pda::main_pool_authority_seeds!(bump_seed);

    let pool = get_associated_token_address(&authority, &mint_address);

    let transfer = SplToken::Transfer(Transfer {
        authority,
        seeds: Seeds::new(authority_seeds),
        source: pool,
        target: target_token,
        amount: spl_amount.as_u64(),
    });

    state.burn(source, chain_id, value).await?;
    state.queue_invoke(transfer).await
}
