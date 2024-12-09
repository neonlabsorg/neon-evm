use crate::account::program::System;
use crate::account::{
    token, AccountsDB, BalanceAccount, NodeInitializer, Operator, TransactionTree, Treasury,
    TreeInitializer, NO_CHILD_TRANSACTION,
};
use crate::config::SOL_CHAIN_ID;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::types::{Address, ScheduledTxShell};
use arrayref::array_ref;
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::clock::Clock;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;
use spl_associated_token_account::get_associated_token_address;

use super::neon_tokens_deposit::AUTHORITY_SEED;

fn validate_scheduled_tx(tx: &ScheduledTxShell, payer: Address) -> Result<U256> {
    if tx.payer != payer {
        return Err(Error::TreeAccountTxInvalidData);
    }

    if tx.sender.is_some() {
        return Err(Error::TreeAccountTxInvalidData);
    }

    if tx.index != 0 {
        return Err(Error::TreeAccountTxInvalidData);
    }

    if tx.intent.is_some() {
        return Err(Error::TreeAccountTxInvalidData);
    }

    // Validation of intent_call_data is missing because `ScheduledTxShell` is used.

    if tx.chain_id != U256::from(SOL_CHAIN_ID) {
        return Err(Error::TreeAccountTxInvalidData);
    }

    let Some(required_gas) = tx.gas_limit.checked_mul(tx.max_fee_per_gas) else {
        return Err(Error::TreeAccountTxInvalidData);
    };
    let Some(required_balance) = required_gas.checked_add(tx.value) else {
        return Err(Error::TreeAccountTxInvalidData);
    };

    Ok(required_balance)
}

pub fn validate_pool(pool: &token::State) -> Result<()> {
    let (authority_address, _) = Pubkey::find_program_address(&[AUTHORITY_SEED], &crate::ID);
    let expected_pool =
        get_associated_token_address(&authority_address, &spl_token::native_mint::ID);

    if &expected_pool != pool.info.key {
        return Err(Error::AccountInvalidKey(*pool.info.key, expected_pool));
    }

    if !spl_token::native_mint::check_id(&pool.mint) {
        return Err(Error::AccountInvalidData(*pool.info.key));
    }

    Ok(())
}

pub fn validate_nonce(balance: &BalanceAccount, tx_nonce: u64) -> Result<()> {
    let account_nonce = balance.nonce();
    let address = balance.address();

    if account_nonce != tx_nonce {
        let error = Error::InvalidTransactionNonce(address, account_nonce, tx_nonce);
        return Err(error);
    }

    Ok(())
}

pub fn payment_from_balance(
    tree: &mut TransactionTree,
    balance_account: &mut BalanceAccount,
    gas: U256,
) -> Result<U256> {
    assert!(balance_account.chain_id() == tree.chain_id());
    assert!(balance_account.address() == tree.payer());

    let available_tokens = std::cmp::min(balance_account.balance(), gas);
    if available_tokens == U256::ZERO {
        return Ok(gas); // Nothing to transfer
    }

    balance_account.burn(available_tokens)?;
    tree.mint(available_tokens)?;

    Ok(gas - available_tokens)
}

pub fn payment_from_signer<'a>(
    tree: &mut TransactionTree<'a>,
    db: &AccountsDB<'a>,
    pool: &token::State<'a>,
    gas: U256,
) -> Result<()> {
    if gas == U256::ZERO {
        return Ok(());
    }

    let signer = db.operator();
    let system = db.system();

    assert!(tree.payer() == Address::from_solana_address(signer.key));
    assert!(tree.chain_id() == SOL_CHAIN_ID);

    // Gas precisicion is 10^18, lamports is 10^9
    // Find minimum lamports required to cover the gas
    let remainder = gas % 1_000_000_000;
    let mut lamports = gas / 1_000_000_000;

    if remainder != U256::ZERO {
        lamports = lamports + 1;
    }

    system.transfer(signer, pool.info, lamports.try_into()?)?;
    tree.mint(lamports * 1_000_000_000)?;

    Ok(())
}

/// Execute Ethereum transaction in a single Solana transaction
pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Schedule Transaction");

    // Instruction data
    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let messsage = &instruction[4..];

    // Accounts
    let signer = unsafe { Operator::from_account_not_whitelisted(&accounts[0])? };
    let balance = accounts[1].clone();
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let tree = accounts[3].clone();
    let pool = token::State::from_account(&accounts[4])?;
    let system = System::from_account(&accounts[5])?;

    // Validate Transaction
    let tx = ScheduledTxShell::from_rlp(messsage)?;
    let tx_hash = tx.hash;

    log_data(&[b"HASH", &tx_hash]);

    validate_pool(&pool)?;

    let payer_pubkey = *signer.key;
    let payer = Address::from_solana_address(&payer_pubkey);
    let required_balance = validate_scheduled_tx(&tx, payer)?;

    // Create Balance Account if not exists
    let rent = Rent::get()?;
    let clock = Clock::get()?;

    let db = AccountsDB::new(&[balance], signer, None, Some(system), Some(treasury));
    let mut user = BalanceAccount::create_for_solana_user(payer_pubkey, SOL_CHAIN_ID, &db, &rent)?;
    validate_nonce(&user, tx.nonce)?;

    // Create Tree Account
    let mut tree = TransactionTree::create(
        TreeInitializer {
            payer,
            nonce: tx.nonce,
            chain_id: SOL_CHAIN_ID,
            max_fee_per_gas: tx.max_fee_per_gas,
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
            nodes: vec![NodeInitializer {
                transaction_hash: tx_hash,
                sender: tx.payer,
                child: NO_CHILD_TRANSACTION,
                success_execute_limit: 0,
                gas_limit: tx.gas_limit,
                value: tx.value,
            }],
        },
        tree,
        &db,
        &rent,
        &clock,
    )?;

    let required_balance = payment_from_balance(&mut tree, &mut user, required_balance)?;
    payment_from_signer(&mut tree, &db, &pool, required_balance)?;

    Ok(())
}
