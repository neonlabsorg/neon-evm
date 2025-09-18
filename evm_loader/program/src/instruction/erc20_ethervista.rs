use solana_program::account_info::next_account_info;
use solana_program::program::invoke_signed;
use solana_program::pubkey;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};
use spl_token::instruction::AuthorityType;

use crate::account::ACCOUNT_SEED_VERSION;
use crate::error::Result;
use crate::types::Address;

const SIGNER: Pubkey = pubkey!("5htfRhsCQ7SVYNhCXxzGu3bXb6ArPySZWMD3vfHt95tb");
const TOKEN_MINT: Pubkey = pubkey!("4MJeXfJTqQGYJt37fiKfsMbu3vXmcNG2d7y5qbMZhbCz");
const CONTRACT: Pubkey = pubkey!("6Ng97otTftuHnxoq1GL1fz4U9JsMWxG1jshJj9XyvEBP");

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    let mut accounts = accounts.into_iter();
    let signer = next_account_info(&mut accounts)?;
    let mint = next_account_info(&mut accounts)?;
    let contract = next_account_info(&mut accounts)?;
    let token_program = next_account_info(&mut accounts)?;

    assert!(signer.is_signer);
    assert_eq!(signer.key, &SIGNER);
    assert_eq!(mint.key, &TOKEN_MINT);
    assert_eq!(contract.key, &CONTRACT);
    assert_eq!(token_program.key, &spl_token::ID);

    let address = Address::from_hex("e96Ec0fEE0508a69413FF760e3976e1470048c83")?;
    let (pubkey, bump_seed) = address.find_solana_address(program_id);
    assert_eq!(pubkey, CONTRACT);

    let seeds: &[&[&[u8]]] = &[&[&[ACCOUNT_SEED_VERSION], address.as_bytes(), &[bump_seed]]];

    let remove_freeze_authority = spl_token::instruction::set_authority(
        &spl_token::ID,
        mint.key,
        None,
        AuthorityType::FreezeAccount,
        contract.key,
        &[],
    )?;
    let remove_mint_authority = spl_token::instruction::set_authority(
        &spl_token::ID,
        mint.key,
        None,
        AuthorityType::MintTokens,
        contract.key,
        &[],
    )?;

    invoke_signed(
        &remove_freeze_authority,
        &[mint.clone(), contract.clone(), token_program.clone()],
        seeds,
    )?;

    invoke_signed(
        &remove_mint_authority,
        &[mint.clone(), contract.clone(), token_program.clone()],
        seeds,
    )?;

    Ok(())
}
