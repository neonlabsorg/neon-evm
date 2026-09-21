use solana_program::{
    account_info::AccountInfo, program::invoke_signed, program_error::ProgramError,
    program_pack::Pack, pubkey, pubkey::Pubkey,
};

use crate::account::{pda, Contract};
use crate::debug::log_data;
use crate::error::{Error, Result};

/// The only account authorized to delete spl-token accounts owned by a contract.
const AUTHORIZED_SIGNER: Pubkey = pubkey!("HdE1e3PAxurMZCi7Wh6zL5XrYtebQYUJCi54mNee2SZX");

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys.into());
    }

    let signer = &accounts[0];
    let contract_info = &accounts[1];
    let token_program = &accounts[2];
    let pool = &accounts[3];
    let token_accounts = &accounts[4..];

    // Verify the signer.
    if *signer.key != AUTHORIZED_SIGNER {
        return Err(Error::AccountInvalidKey(*signer.key, AUTHORIZED_SIGNER));
    }
    if !signer.is_signer {
        return Err(Error::AccountNotSigner(*signer.key));
    }

    if *token_program.key != spl_token::ID {
        return Err(Error::AccountInvalidKey(*token_program.key, spl_token::ID));
    }

    // The contract account is the spl-token authority for its token accounts and
    // signs the CPIs via its PDA seeds.
    let contract = Contract::from_account_info(program_id, contract_info)?;
    let contract_address = contract.address();
    let (authority_key, bump_seed) = pda::contract(program_id, &contract_address);
    if *contract_info.key != authority_key {
        return Err(Error::AccountInvalidKey(*contract_info.key, authority_key));
    }
    let signer_seeds: &[&[u8]] = pda::contract_seeds!(contract_address, bump_seed);

    // The pool must be an spl-token account owned by the signer.
    if !spl_token::check_id(pool.owner) {
        return Err(Error::AccountInvalidOwner(*pool.key, spl_token::ID));
    }
    let pool_token = spl_token::state::Account::unpack(&pool.try_borrow_data()?)?;
    if pool_token.owner != *signer.key {
        return Err(Error::AccountInvalidKey(pool_token.owner, *signer.key));
    }

    for account in token_accounts {
        // Each account must be an spl-token account owned by the contract.
        if !spl_token::check_id(account.owner) {
            return Err(Error::AccountInvalidOwner(*account.key, spl_token::ID));
        }
        let token = spl_token::state::Account::unpack(&account.try_borrow_data()?)?;
        if token.owner != authority_key {
            return Err(Error::AccountInvalidKey(token.owner, authority_key));
        }

        // Transfer the whole balance to the pool.
        if token.amount > 0 {
            log_data(&[
                b"delete_spl_token",
                contract_address.as_bytes(),
                account.key.as_ref(),
                &token.amount.to_le_bytes(),
            ]);

            let transfer = spl_token::instruction::transfer(
                &spl_token::ID,
                account.key,
                pool.key,
                &authority_key,
                &[],
                token.amount,
            )?;
            invoke_signed(&transfer, accounts, &[signer_seeds])?;
        }

        // Close the account, sending the rent lamports to the signer.
        let close = spl_token::instruction::close_account(
            &spl_token::ID,
            account.key,
            signer.key,
            &authority_key,
            &[],
        )?;
        invoke_signed(&close, accounts, &[signer_seeds])?;
    }

    Ok(())
}
