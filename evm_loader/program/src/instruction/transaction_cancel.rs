use crate::account::{AccountsDB, BalanceAccount, Operator, StateAccount};
use crate::error::{Error, Result};
use crate::gasometer::{CANCEL_TRX_COST, LAST_ITERATION_COST};
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    solana_program::msg!("Instruction: Cancel Transaction");

    let storage_info = accounts[0].clone();
    let operator = Operator::from_account(&accounts[1])?;
    let balance = BalanceAccount::from_account(program_id, accounts[2].clone(), None)?;

    let accounts_database = AccountsDB::new(&accounts[3..], operator, Some(balance), None, None);

    let storage = StateAccount::restore(program_id, storage_info, &accounts_database, true)?;

    let transaction_hash = array_ref![instruction, 0, 32];

    solana_program::log::sol_log_data(&[b"HASH", transaction_hash]);

    validate(&storage, transaction_hash)?;
    execute(program_id, accounts_database, storage)
}

fn validate(storage: &StateAccount, transaction_hash: &[u8; 32]) -> Result<()> {
    if &storage.trx_hash() != transaction_hash {
        return Err(Error::HolderInvalidHash(
            storage.trx_hash(),
            *transaction_hash,
        ));
    }

    Ok(())
}

fn execute<'a>(
    program_id: &Pubkey,
    mut accounts: AccountsDB<'a>,
    mut storage: StateAccount<'a>,
) -> Result<()> {
    let used_gas = U256::ZERO;
    let total_used_gas = storage.gas_used();
    solana_program::log::sol_log_data(&[
        b"GAS",
        &used_gas.to_le_bytes(),
        &total_used_gas.to_le_bytes(),
    ]);

    let gas = U256::from(CANCEL_TRX_COST + LAST_ITERATION_COST);
    let _ = storage.consume_gas(gas, accounts.operator_balance()); // ignore error

    let origin = storage.trx_origin();
    let (origin_pubkey, _) = origin.find_balance_address(program_id, storage.trx_chain_id());

    {
        let origin_info = accounts.get(&origin_pubkey).clone();
        let mut account = BalanceAccount::from_account(program_id, origin_info, Some(origin))?;
        account.increment_nonce()?;

        storage.refund_unused_gas(&mut account)?;
    }

    storage.finalize(program_id, &accounts)
}
