use crate::account::{AccountsDB, StateAccount, TransactionTree};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::error::Result;
use crate::gasometer::Gasometer;
use crate::instruction::transaction_internals::{allocate_evm, finalize};

pub fn do_scheduled_start<'a>(
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'a>,
    mut transaction_tree: TransactionTree<'a>,
    gasometer: Gasometer,
) -> Result<()> {
    debug_print!("do_scheduled_start");

    let mut account_storage = ProgramAccountStorage::new(accounts)?;

    let origin = storage.trx_origin();

    storage.trx().validate(origin, &account_storage)?;

    // Increment origin's nonce only once for the whole execution tree.
    let mut origin_account = account_storage.origin(origin, storage.trx())?;
    if origin_account.nonce() == storage.trx().nonce() {
        origin_account.increment_revision(account_storage.rent(), account_storage.db())?;
        origin_account.increment_nonce()?;
    }

    // Burn `gas_limit` tokens (both base fee and priority, if any) from the tree account.
    // Later we will mint them to the operator.
    // Remaining tokens are returned back to the tree account in the last iteration.
    let gas_limit_in_tokens = storage.trx().gas_limit_in_tokens()?;
    let max_priority_fee_in_tokens = storage.trx().priority_fee_limit_in_tokens()?;
    transaction_tree.burn(gas_limit_in_tokens + max_priority_fee_in_tokens)?;

    transaction_tree.start_transaction(storage.trx())?;

    allocate_evm(&mut account_storage, &mut storage)?;
    let mut state_data = storage.read_executor_state();

    let (_, touched_accounts) = state_data.deconstruct();
    finalize(
        0,
        storage,
        account_storage,
        None,
        gasometer,
        touched_accounts,
    )
}
