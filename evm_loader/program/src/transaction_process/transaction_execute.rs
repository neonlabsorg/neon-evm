use crate::account::AllocateResult;
use crate::allocator::StateAllocator;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::{ExitStatus, Machine};
use crate::executor::{ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::platform::{Platform, Solana};
use crate::types::{Address, Transaction};

pub fn execute(
    mut solana: Solana,
    trx: Transaction,
    origin: Address,
    allocator: StateAllocator,
) -> Result<()> {
    trx.validate(origin, &solana, None)?;
    solana.get_origin((origin, &trx))?.increment_nonce()?;

    let mut backend_data = ExecutorStateData::new_in(allocator);
    let mut backend = ExecutorState::new_in(&mut solana, &mut backend_data, allocator);

    let (exit_reason, steps_executed) = {
        let mut evm = Machine::new_in(&trx, origin, &mut backend, allocator)?;
        evm.execute(u64::MAX, &mut backend)?
    };

    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(), // Iteration steps
        &steps_executed.to_le_bytes(), // Total steps is the same as iteration steps
    ]);

    let allocate_result = backend.allocate_state_in_solana()?;
    if allocate_result != AllocateResult::Ready {
        return Err(Error::AccountSpaceAllocationFailure);
    }

    backend.commit_state_to_solana()?;

    solana.update_accounts_lamports()?;
    solana.use_gasometer(|g| g.record_solana_transaction_cost(trx.gas_limit()))?;
    solana.reward_operator_from_origin(origin, &trx)?;

    log_return_value(&exit_reason);
    Ok(())
}

pub fn execute_with_solana_call(
    mut solana: Solana,
    trx: Transaction,
    origin: Address,
    allocator: StateAllocator,
) -> Result<()> {
    trx.validate(origin, &solana, None)?;
    solana.get_origin((origin, &trx))?.increment_nonce()?;

    let mut backend_data = ExecutorStateData::new_in(allocator);
    let mut backend = SyncedExecutorState::new_in(&mut solana, &mut backend_data, allocator);

    let (exit_reason, steps_executed) = {
        let mut evm = Machine::new_in(&trx, origin, &mut backend, allocator)?;
        evm.execute(u64::MAX, &mut backend)?
    };

    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(), // Iteration steps
        &steps_executed.to_le_bytes(), // Total steps is the same as iteration steps
    ]);

    backend.commit_timestamps_to_solana()?;

    solana.update_accounts_lamports()?;
    solana.use_gasometer(|g| g.record_solana_transaction_cost(trx.gas_limit()))?;
    solana.reward_operator_from_origin(origin, &trx)?;

    log_return_value(&exit_reason);
    Ok(())
}

fn log_return_value(status: &ExitStatus) {
    let code: u8 = status.code();

    log_msg!("exit_status={:#04X}", code); // Tests compatibility
    if let ExitStatus::Revert(msg) = status {
        crate::error::print_revert_message(msg);
    }

    log_data(&[b"RETURN", &[code]]);
}
