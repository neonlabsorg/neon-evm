use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::{pda, token, Operator};
use crate::config::CHAIN_ID_LIST;
use crate::error::{Error, Result};
use crate::executor::external_programs::{get_associated_token_address, spl_token, Invokable};
use crate::platform::{Platform, Solana};
use crate::types::seeds::Seeds;
use crate::types::Address;

struct Accounts<'r, 'a> {
    mint: token::Mint<'a>,
    source: token::State<'a>,
    pool: token::State<'a>,
    balance_account: AccountInfo<'a>,
    operator: Operator<'a>,
    all: &'r [AccountInfo<'a>],
}

impl<'r, 'a> Accounts<'r, 'a> {
    pub fn from_slice(accounts: &'r [AccountInfo<'a>]) -> Result<Accounts<'r, 'a>> {
        Ok(Accounts {
            mint: token::Mint::from_account_info(&accounts[0])?,
            source: token::State::from_account_info(&accounts[1])?,
            pool: token::State::from_account_info(&accounts[2])?,
            balance_account: accounts[3].clone(),
            // contract_account: &accounts[4],
            // token_program: &accounts[5],
            operator: unsafe { Operator::from_account_not_whitelisted(&accounts[6]) }?,
            all: accounts,
        })
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Deposit");

    let parsed_accounts = Accounts::from_slice(accounts)?;

    let address = array_ref![instruction, 0, 20];
    let address = Address::from(*address);

    let chain_id = array_ref![instruction, 20, 8];
    let chain_id = u64::from_le_bytes(*chain_id);

    validate(program_id, &parsed_accounts, address, chain_id)?;
    execute(program_id, parsed_accounts, address, chain_id)
}

fn validate(
    program_id: &Pubkey,
    accounts: &Accounts,
    address: Address,
    chain_id: u64,
) -> Result<()> {
    let balance_account = *accounts.balance_account.key;
    let pool = *accounts.pool.info.key;
    let mint = *accounts.mint.info.key;

    let (expected_pubkey, _) = pda::balance(&program_id, &address, chain_id);
    if expected_pubkey != balance_account {
        return Err(Error::AccountInvalidKey(balance_account, expected_pubkey));
    }

    let Ok(chain_id_index) = CHAIN_ID_LIST.binary_search_by_key(&chain_id, |c| c.0) else {
        return Err(Error::InvalidChainId(chain_id));
    };

    let expected_mint = CHAIN_ID_LIST[chain_id_index].2;
    if mint != expected_mint {
        return Err(Error::AccountInvalidKey(mint, expected_mint));
    }

    let (authority_address, _) = pda::main_pool_authority(program_id);
    let expected_pool = get_associated_token_address(&authority_address, &mint);
    if pool != expected_pool {
        return Err(Error::AccountInvalidKey(pool, expected_pool));
    }

    if (accounts.pool.mint() != mint) || (accounts.source.mint() != mint) {
        return Err(Error::from("Invalid token mint"));
    }

    let delegate = accounts.source.delegate();
    if delegate != Some(accounts.balance_account.key) {
        return Err(Error::from("Expected tokens delegated to balance account"));
    }

    if accounts.source.delegated_amount() < 1 {
        return Err(Error::from("Expected positive tokens amount delegated"));
    }

    Ok(())
}

fn execute(program_id: &Pubkey, accounts: Accounts, address: Address, chain_id: u64) -> Result<()> {
    let mut solana = Solana::new(accounts.all, accounts.operator, None)?;

    let (authority, bump_seed) = pda::balance(program_id, &address, chain_id);
    let signer_seeds: &[&[u8]] = pda::balance_seeds!(&address, chain_id, bump_seed);

    let delegated_amount = accounts.source.delegated_amount();

    spl_token::Transfer {
        authority,
        seeds: Seeds::new(signer_seeds),
        source: accounts.source.pubkey(),
        target: accounts.pool.pubkey(),
        amount: delegated_amount,
    }
    .invoke(&mut solana)?;

    let token_decimals = accounts.mint.decimals();
    assert!(token_decimals <= 18);

    let additional_decimals: u32 = (18 - token_decimals).into();
    let deposit = U256::from(delegated_amount) * 10_u128.pow(additional_decimals);

    let mut balance_account = solana.create_balance(address, chain_id)?;
    balance_account.mint(deposit)?;

    solana.update_accounts_lamports()
}
