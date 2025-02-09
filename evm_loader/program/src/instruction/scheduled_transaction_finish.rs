use crate::account::{
    Operator, OperatorBalanceAccount, OperatorBalanceValidator, StateAccount, TransactionTree,
};
use crate::config::TREE_ACCOUNT_FINISH_TRANSACTION_GAS;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::ExitStatus;
use crate::executor::ExecutorStateData;
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
    let mut operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    let mut state = StateAccount::restore_without_revision_check(program_id, &storage_info)?;
    let mut executor_state = state.read_executor_state();
    let trx = state.trx();

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&trx)?;
    let miner_address = operator_balance.miner(state.trx_origin());

    log_data(&[b"HASH", &trx.hash]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    // Validate.
    let (index, exit_status) = validate(&mut executor_state, &state, trx, &transaction_tree)?;

    // Handle gas, transaction costs to operator, refund into tree account.
    const GAS: U256 = U256::new(TREE_ACCOUNT_FINISH_TRANSACTION_GAS as u128);
    if let Some(operator_balance) = &mut operator_balance {
        // don't burn tokens in tree, because it was already reserved at the start
        operator_balance.mint(GAS)?;
    }

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
    let (results, _, _) = executor_state.deconstruct();
    let (exit_status, _) = results.ok_or(Error::ScheduledTxNoExitStatus(*state.account_key()))?;

    let index = trx.if_scheduled().map(|t| t.index).unwrap();

    Ok((index, exit_status))
}
