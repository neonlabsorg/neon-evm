use solana_program::{
    account_info::AccountInfo, program::invoke_signed, program_error::ProgramError, pubkey,
    pubkey::Pubkey,
};
use spl_associated_token_account::get_associated_token_address;

use crate::account::{pda, program, token};
use crate::error::{Error, Result};

/// The only account authorized to delete a deposit pool.
const AUTHORIZED_SIGNER: Pubkey = pubkey!("HdE1e3PAxurMZCi7Wh6zL5XrYtebQYUJCi54mNee2SZX");

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    if accounts.len() < 5 {
        return Err(ProgramError::NotEnoughAccountKeys.into());
    }

    let signer = &accounts[0];
    let destination = token::State::from_account_info(&accounts[1])?;
    let pool = token::State::from_account_info(&accounts[2])?;
    let authority = &accounts[3];
    let token_program = program::Token::from_account(&accounts[4])?;

    // Verify the signer.
    if *signer.key != AUTHORIZED_SIGNER {
        return Err(Error::AccountInvalidKey(*signer.key, AUTHORIZED_SIGNER));
    }
    if !signer.is_signer {
        return Err(Error::AccountNotSigner(*signer.key));
    }

    // The pool authority PDA signs the spl-token CPIs.
    let (authority_key, bump_seed) = pda::main_pool_authority(program_id);
    if *authority.key != authority_key {
        return Err(Error::AccountInvalidKey(*authority.key, authority_key));
    }
    let authority_seeds: &[&[u8]] = pda::main_pool_authority_seeds!(bump_seed);

    // The pool must be the deposit pool (associated token account of the authority) for its mint.
    let expected_pool = get_associated_token_address(&authority_key, &pool.mint);
    if *pool.info.key != expected_pool {
        return Err(Error::AccountInvalidKey(*pool.info.key, expected_pool));
    }

    // The destination must be an spl-token account owned by the signer with the same mint.
    if destination.owner != *signer.key {
        return Err(Error::AccountInvalidKey(destination.owner, *signer.key));
    }
    if destination.mint != pool.mint {
        return Err(Error::from("Invalid token mint"));
    }

    // Transfer the whole pool balance to the destination.
    if pool.amount > 0 {
        let transfer = spl_token::instruction::transfer(
            token_program.key,
            pool.info.key,
            destination.info.key,
            authority.key,
            &[],
            pool.amount,
        )?;
        let account_infos = &[
            pool.info.clone(),
            destination.info.clone(),
            authority.clone(),
            token_program.clone(),
        ];
        invoke_signed(&transfer, account_infos, &[authority_seeds])?;
    }

    // Close the pool account, sending the rent lamports to the signer.
    let close = spl_token::instruction::close_account(
        token_program.key,
        pool.info.key,
        signer.key,
        authority.key,
        &[],
    )?;
    let account_infos = &[
        pool.info.clone(),
        signer.clone(),
        authority.clone(),
        token_program.clone(),
    ];
    invoke_signed(&close, account_infos, &[authority_seeds])?;

    Ok(())
}
