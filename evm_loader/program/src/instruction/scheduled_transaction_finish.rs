use crate::account::{
    AccountsDB, Operator, OperatorBalanceAccount, OperatorBalanceValidator, StateAccount,
    TransactionTree,
};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::ExitStatus;
use crate::executor::ExecutorStateData;
use crate::gasometer::SCHEDULED_FINISH_COST;
use crate::instruction::priority_fee_txn_calculator;
use crate::types::Transaction;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Finalize Scheduled Transaction");

    let storage_info = accounts[0].clone();
    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;
    let operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    let accounts_db = AccountsDB::new(&[], operator, operator_balance.clone(), None, None);

    let (mut state, _) = StateAccount::restore(program_id, &storage_info, &accounts_db)?;
    let mut executor_state = state.read_executor_state();
    let trx = state.trx();

    operator_balance.validate_transaction(&trx)?;
    let miner_address = operator_balance.miner(state.trx_origin());

    log_data(&[b"HASH", &trx.hash]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    // Validate.
    let (index, exit_status) = validate(&mut executor_state, &state, trx, &transaction_tree)?;

    // Handle gas, transaction costs to operator, refund into tree account.
    let gas = U256::from(SCHEDULED_FINISH_COST);
    let priority_fee = priority_fee_txn_calculator::handle_priority_fee(state.trx(), gas)?;
    let _ = state.consume_gas(gas, priority_fee, accounts_db.try_operator_balance()); // ignore error

    let refund = state.materialize_unused_gas()?;
    transaction_tree.mint(refund)?;

    // Finalize.
    transaction_tree.end_transaction(index, exit_status)?;
    state.finish_scheduled_tx(program_id)?;

    Ok(())
}

fn validate<'a>(
    executor_state: &'a mut ExecutorStateData,
    state: &StateAccount,
    trx: &Transaction,
    tree: &TransactionTree,
) -> Result<(u16, &'a ExitStatus)> {
    // Validate if it's a scheduled transaction at all.
    if !trx.is_scheduled_tx() {
        return Err(Error::NotScheduledTransaction);
    }

    // Validate if the tree account is the one we used at the transaction start.
    let trx_tree_account = state
        .tree_account()
        .expect("Unreachable code path: validation in the State Account contains a bug.");
    let actual_tree_pubkey = *tree.info().key;
    if trx_tree_account != actual_tree_pubkey {
        return Err(Error::ScheduledTxInvalidTreeAccount(
            trx_tree_account,
            actual_tree_pubkey,
        ));
    }

    // Validate and get the exit_status and index.
    let (results, _) = executor_state.deconstruct();
    let (exit_status, _) = results.ok_or(Error::ScheduledTxNoExitStatus(*state.account_key()))?;

    let index = trx.if_scheduled().map(|t| t.index).unwrap();

    Ok((index, exit_status))
}
