use std::u64;

use crate::account::{AllocateResult, BalanceAccount, Root, StateAccount};

use crate::config::EVM_STEPS_MIN;
use crate::debug::log_data;
use crate::error::{build_revert_message, Error, Result};
use crate::evm::ExitStatus;
use crate::executor::precompile_extension::call_solana;
use crate::executor::{ExecutorState, SyncedExecutorState};
use crate::platform::{Platform, Solana};

fn commit_state_to_solana(solana: &mut Solana, root: &mut Root) -> Result<()> {
    let allocator = root.allocator;
    let mut executor = SyncedExecutorState::new_in(solana, &mut root.executor_state, allocator);

    let allocate_result = executor.allocate_state_in_solana()?;
    if allocate_result != AllocateResult::Ready {
        return Err(Error::AccountSpaceAllocationFailure);
    }

    executor.commit_actions_to_solana()?;
    executor.commit_timestamps_to_solana()?;

    Ok(())
}

fn run_evm(solana: &mut Solana, root: &mut Root, step_limit: u64) -> Result<()> {
    if (step_limit < EVM_STEPS_MIN) && (root.gas_price() > 0) {
        return Err(Error::StepLimitBellowMinimum(step_limit, EVM_STEPS_MIN));
    }

    let allocator = root.allocator;

    let mut executor = ExecutorState::new_in(solana, &mut root.executor_state, allocator);

    let evm = &mut root.machine_state;
    let (exit_status, steps_executed) = evm.execute(step_limit, &mut executor)?;

    let touched_accounts = executor.into_touched_accounts();

    if let ExitStatus::Interrupted(interrupt) = exit_status {
        root.set_interupted_state(interrupt);
    } else if ExitStatus::StepLimit != exit_status {
        root.set_exit_status(&exit_status);
    }

    root.increment_steps_executed(steps_executed)?;
    root.update_touched_accounts(touched_accounts, solana)?;

    Ok(())
}

fn run_evm_interrupted(solana: &mut Solana, root: &mut Root) -> Result<()> {
    let allocator = root.allocator;

    // Switch to the solana_call panic mode
    solana.panic_on_revert();

    // If the execution was interrupted, we need to commit the state first
    // Try to finish the rest of the execution in the current Solana transaction
    commit_state_to_solana(solana, root)?;

    let Some(interrupt) = &root.interrupted_state else {
        unreachable!();
    };

    let mut executor = SyncedExecutorState::new_in(solana, &mut root.executor_state, allocator);
    let evm = &mut root.machine_state;

    // Start from interrupted CPI from the previous iteration
    // After the CPI is executed, exit from the precompile stack frame
    match call_solana::execute_from_interrupt(&mut executor, evm.context(), interrupt) {
        Ok(return_data) => {
            evm.return_from_stack_frame(&return_data, &mut executor)?;
            evm.increment_pc();
        }
        Err(error) => {
            evm.revert_from_stack_frame(error, &mut executor)?;
            evm.increment_pc();
        }
    }

    // Execute the EVM till the end
    let (exit_status, steps_executed) = evm.execute(u64::MAX, &mut executor)?;

    // EVM was executed with a synced state, so it is already commited to Solana
    // We only need to update contracts timestamps
    executor.commit_timestamps_to_solana()?;

    root.set_exit_status(&exit_status);
    root.increment_steps_executed(steps_executed)?;

    Ok(())
}

pub fn start_iterative(mut solana: Solana, mut state: StateAccount) -> Result<()> {
    let tx_hash = state.transaction_hash();
    let mut root = state.root_mut();

    let mut origin: BalanceAccount = solana.get_origin(&*root)?;
    origin.increment_nonce()?;

    // All state changes are happened in the `StateAccount` and `Root` constructors
    // Do almost nothing here, so we can guarantee that the first solana transaction succeeds

    solana.log_miner_address(root.origin());
    log_data(&[b"HASH", tx_hash.as_ref()]);

    // Still print steps into the logs
    root.increment_steps_executed(0)?;

    // Spend gas
    solana.update_accounts_lamports()?;
    solana.use_gasometer(|g| g.record_solana_transaction_cost(root.gas_limit()))?;
    solana.reward_operator_from_holder(&mut root)
}

pub fn execute_iterative(
    mut solana: Solana,
    mut state: StateAccount,
    step_limit: u64,
) -> Result<()> {
    let tx_hash = state.transaction_hash();
    let mut root = state.root_mut();

    solana.log_miner_address(root.origin());
    log_data(&[b"HASH", tx_hash.as_ref()]);

    if root.is_execution_finished() {
        root.increment_steps_executed(0)?; // Still print steps into the logs

        // Already finished, commit the state in multiple iterations
        match commit_state_to_solana(&mut solana, &mut root) {
            Ok(_) | Err(Error::AccountSpaceAllocationFailure) => {}
            Err(e) => return Err(e),
        }
    } else if root.is_execution_interrupted() {
        // Previous iteration was interrupted with a CPI call
        run_evm_interrupted(&mut solana, &mut root)?;
    } else {
        // Normal execution, run EVM with the given step limit
        run_evm(&mut solana, &mut root, step_limit)?;
    };

    // Spend gas
    solana.update_accounts_lamports()?;
    solana.use_gasometer(|g| g.record_solana_transaction_cost(root.gas_limit()))?;
    solana.reward_operator_from_holder(&mut root)?;

    if root.can_be_finalized() {
        // For scheduled transactions refund unused gas later to the tree account in `scheduled_transaction_finish`
        // Because the tree account is not available here
        if !root.is_scheduled_transaction() {
            root.refund_unused_gas_to_origin(&mut solana)?;
        }

        // The transaction is fully comitted to Solana
        // We can finalize the Holder account
        log_return_value(&root);

        std::mem::drop(root);
        state.finalize()?;
    }

    Ok(())
}

// Mostly the same as in `transaction_execute.rs`
fn log_return_value(root: &Root) {
    assert!(root.is_execution_finished());

    log_msg!("exit_status={:#04X}", root.exit_status_code); // Tests compatibility
    if let Some(ref msg) = root.revert_message {
        crate::error::print_revert_message(msg);
    }

    log_data(&[b"RETURN", &[root.exit_status_code]]);
}
