use crate::account::{AccountsDB, StateAccount, TransactionTree};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::{allocate_evm, finalize};
use crate::types::{ScheduledTx, Transaction, TrxView};

pub fn do_scheduled_start<'a, 'b>(
    trx: &Transaction,
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'b>,
    mut transaction_tree: TransactionTree<'a>,
    mut gasometer: Gasometer,
) -> Result<()>
where
    'a: 'b,
{
    debug_print!("do_scheduled_start");

    let mut account_storage = ProgramAccountStorage::new(accounts)?;

    let origin = storage.trx_origin();

    trx.validate(origin, &account_storage, Some(&transaction_tree))?;

    transaction_tree.start_transaction(trx)?;

    // Increment origin's nonce only once for the whole execution tree.
    let mut origin_account = account_storage.origin(origin, trx)?;
    if origin_account.nonce() == trx.nonce() {
        origin_account.increment_revision(account_storage.rent(), account_storage.db())?;
        origin_account.increment_nonce()?;
    }

    // Burn `gas_limit` tokens from the tree account.
    // Later we will mint them to the operator.
    // Remaining tokens are returned back to the tree account in the last iteration.
    let gas_limit_in_tokens = trx.gas_limit_in_tokens()?;
    transaction_tree.burn(gas_limit_in_tokens)?;

    // record gas for the future finish
    gasometer.record_scheduled_transaction_finish();

    allocate_evm(trx, &mut account_storage, &mut storage)?;
    finalize(0, storage, account_storage, gasometer, false, None)
}

pub fn validate_scheduled_tx<'a>(
    trx: &'a Transaction,
    instruction_index: u16,
) -> Result<&'a ScheduledTx> {
    // Validate that it's indeed a scheduled tx.
    if !trx.is_scheduled_tx() {
        return Err(Error::NotScheduledTransaction);
    }

    let scheduled_trx = trx.if_scheduled().unwrap();
    let trx_index = trx.tree_account_index().unwrap();
    if trx_index == instruction_index {
        Ok(scheduled_trx)
    } else {
        Err(Error::ScheduledTxInvalidIndex(trx_index, instruction_index))
    }
    // Validation that the given transaction corresponds to the node in the tree account
    // is happening inside the tree account.
}
