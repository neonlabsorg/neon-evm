use crate::account::{AccountsDB, AllocateResult};
use crate::account_storage::ProgramAccountStorage;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::Machine;
use crate::executor::{ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::{handle_gas, log_return_value};
use crate::types::{boxx::Boxx, Address, Transaction};

pub fn execute(
    accounts: AccountsDB<'_>,
    gasometer: Gasometer,
    trx: Boxx<Transaction>,
    origin: Address,
) -> Result<()> {
    let mut account_storage = ProgramAccountStorage::new(accounts)?;
    let mut backend_data = ExecutorStateData::new(&account_storage);

    trx.validate(origin, &account_storage, None)?;

    account_storage.origin(origin, &trx)?.increment_nonce()?;

    let (exit_reason, steps_executed) = {
        let mut backend = ExecutorState::new(&mut account_storage, &mut backend_data);

        let mut evm = Machine::new(&trx, origin, &mut backend, None::<NoopEventListener>)?;
        let (result, steps_executed, _, _) = evm.execute(u64::MAX, &mut backend)?;

        (result, steps_executed)
    };

    let apply_state = backend_data.into_actions();

    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(), // Iteration steps
        &steps_executed.to_le_bytes(), // Total steps is the same as iteration steps
    ]);

    let allocate_result = account_storage.allocate(apply_state)?;
    if allocate_result != AllocateResult::Ready {
        return Err(Error::AccountSpaceAllocationFailure);
    }

    account_storage.apply_state_change(apply_state)?;
    account_storage.transfer_treasury_payment()?;

    handle_gas(&mut account_storage, &trx, gasometer, origin)?;

    log_return_value(&exit_reason);
    Ok(())
}

pub fn execute_with_solana_call(
    accounts: AccountsDB<'_>,
    gasometer: Gasometer,
    trx: Boxx<Transaction>,
    origin: Address,
) -> Result<()> {
    let mut account_storage = ProgramAccountStorage::new(accounts)?;

    trx.validate(origin, &account_storage, None)?;

    account_storage.origin(origin, &trx)?.increment_nonce()?;

    let (exit_reason, steps_executed) = {
        let mut backend = SyncedExecutorState::new(&mut account_storage);

        let mut evm = Machine::new(&trx, origin, &mut backend, None::<NoopEventListener>)?;
        let (result, steps_executed, _, _) = evm.execute(u64::MAX, &mut backend)?;

        (result, steps_executed)
    };

    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(), // Iteration steps
        &steps_executed.to_le_bytes(), // Total steps is the same as iteration steps
    ]);

    account_storage.increment_revision_for_modified_contracts()?;
    account_storage.transfer_treasury_payment()?;

    handle_gas(&mut account_storage, &trx, gasometer, origin)?;

    log_return_value(&exit_reason);
    Ok(())
}
