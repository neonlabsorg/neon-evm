use crate::account::{AccountsDB, AllocateResult};
use crate::account_storage::ProgramAccountStorage;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::Machine;
use crate::executor::{ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::log_return_value;
use crate::instruction::priority_fee_txn_calculator;
use crate::types::{boxx::Boxx, Address, Transaction};
use ethnum::U256;

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
    let timestamped_contracts = backend_data.timestamped_contracts.take();

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
    account_storage.update_timestamped_contracts(timestamped_contracts.keys())?;
    account_storage.transfer_treasury_payment()?;

    handle_gas(account_storage, &trx, gasometer, origin)?;

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

    handle_gas(account_storage, &trx, gasometer, origin)?;

    log_return_value(&exit_reason);
    Ok(())
}

fn handle_gas(
    mut account_storage: ProgramAccountStorage,
    trx: &Transaction,
    mut gasometer: Gasometer,
    origin: Address,
) -> Result<()> {
    let gas_limit = trx.gas_limit();
    let gas_price = trx.gas_price();
    let chain_id = trx.chain_id().unwrap_or(crate::config::DEFAULT_CHAIN_ID);

    gasometer.record_operator_expenses(account_storage.operator());
    let used_gas = gasometer.used_gas();
    if used_gas > gas_limit {
        return Err(Error::OutOfGas(gas_limit, used_gas));
    }

    log_data(&[b"GAS", &used_gas.to_le_bytes(), &used_gas.to_le_bytes()]);

    let gas_cost = used_gas.saturating_mul(gas_price);
    let priority_fee =
        priority_fee_txn_calculator::finalize_priority_fee(&trx, used_gas, U256::ZERO)?;
    account_storage.transfer_gas_payment(origin, chain_id, gas_cost + priority_fee)?;

    Ok(())
}
