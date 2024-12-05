use solana_program::pubkey::Pubkey;

use crate::account::{AccountsDB, AllocateResult, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::config::{EVM_STEPS_LAST_ITERATION_MAX, EVM_STEPS_MIN};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::{ExitStatus, Machine};
use crate::executor::{Action, ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::gasometer::{Gasometer, LAMPORTS_PER_SIGNATURE};
use crate::instruction::priority_fee_txn_calculator;
use crate::types::boxx::boxx;
use crate::types::TreeMap;
use crate::types::Vector;

use crate::executor::precompile_extension::call_solana::execute_external_instruction;
//use crate::evm::opcode;
use solana_program::instruction::Instruction;



type SyncedEvmBackend<'a, 'r> = SyncedExecutorState<'r, ProgramAccountStorage<'a>>;
type EvmBackend<'a, 'r> = ExecutorState<'r, ProgramAccountStorage<'a>>;
type Evm<'a, 'r> = Machine<EvmBackend<'a, 'r>, NoopEventListener>;

pub fn do_begin<'a>(
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'a>,
    gasometer: Gasometer,
) -> Result<()> {
    debug_print!("do_begin");

    let mut account_storage = ProgramAccountStorage::new(accounts)?;

    let origin = storage.trx_origin();

    storage.trx().validate(origin, &account_storage)?;

    // Increment origin nonce in the first iteration
    // This allows us to run multiple iterative transactions from the same sender in parallel
    // These transactions are guaranteed to start in a correct sequence
    // BUT they finalize in an undefined order
    let mut origin_account = account_storage.origin(origin, storage.trx())?;
    origin_account.increment_revision(account_storage.rent(), account_storage.db())?;
    origin_account.increment_nonce()?;

    // Burn `gas_limit` tokens (both base fee and priority, if any) from the origin account.
    // Later we will mint them to the operator.
    // Remaining tokens are returned to the origin in the last iteration.
    let gas_limit_in_tokens = storage.trx().gas_limit_in_tokens()?;
    let max_priority_fee_in_tokens = storage.trx().priority_fee_limit_in_tokens()?;
    origin_account.burn(gas_limit_in_tokens + max_priority_fee_in_tokens)?;

    allocate_or_reinit_state(&mut account_storage, &mut storage, true)?;
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

pub fn do_continue<'a>(
    step_count: u64,
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'a>,
    mut gasometer: Gasometer,
    reset: bool,
) -> Result<()> {
    debug_print!("do_continue");

    if (step_count < EVM_STEPS_MIN) && (storage.trx().gas_price() > 0) {
        return Err(Error::Custom(format!(
            "Step limit {step_count} below minimum {EVM_STEPS_MIN}"
        )));
    }
    if reset {
        log_data(&[b"RESET"]);
    }
    let mut account_storage = ProgramAccountStorage::new(accounts)?;
    allocate_or_reinit_state(&mut account_storage, &mut storage, reset)?;

    let mut state_data = storage.read_executor_state();
    if storage.steps_interrupted() > 0 {
        account_storage.apply_state_change(state_data.into_actions())?;
        return finalize_interrupted(
            &mut account_storage,
            &mut storage,
            &mut gasometer,
            &state_data,
        );
    }
    let mut evm = storage.read_evm::<EvmBackend, NoopEventListener>();
    let mut backend = ExecutorState::new(&mut account_storage, &mut state_data);
    let mut steps_executed = 0;

    if backend.exit_status().is_none() {
        let (exit_status, steps_returned, _, _) = evm.execute(step_count, &mut backend)?;
        if exit_status == ExitStatus::Interrupted {
            storage.increment_steps_interrupted(1)?;
        }
        if exit_status != ExitStatus::StepLimit && exit_status != ExitStatus::Interrupted {
            backend.set_exit_status(exit_status)
        }

        steps_executed = steps_returned;
    }

    let (mut results, touched_accounts) = state_data.deconstruct();
    if steps_executed > EVM_STEPS_LAST_ITERATION_MAX {
        results = None;
    }

    finalize(
        steps_executed,
        storage,
        account_storage,
        results,
        gasometer,
        touched_accounts,
    )
}

fn allocate_or_reinit_state(
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
    is_allocate: bool,
) -> Result<()> {
    if is_allocate {
        storage.reset_steps_executed();
        storage.reset_steps_interrupted();

        // Dealloc evm that was potentially alloced in previous iterations before the reset.
        if storage.is_evm_alloced() {
            storage.dealloc_evm::<EvmBackend, NoopEventListener>();
        }

        // Dealloc executor state that was potentially alloced in previous iterations before the reset.
        // Also, copy the block params for use into the new ExecutorStateData.
        let mut block_params = None;
        if storage.is_executor_state_alloced() {
            block_params = Some(storage.read_executor_state().get_block_params());
            storage.dealloc_executor_state();
        }

        let mut state_data = {
            // Preserve the previous block params.
            if block_params.is_some() {
                boxx(ExecutorStateData::new_with_block_params(
                    block_params.unwrap(),
                ))
            } else {
                boxx(ExecutorStateData::new(account_storage))
            }
        };
        let mut evm_backend = ExecutorState::new(account_storage, &mut state_data);
        let evm = boxx(Evm::new(
            storage.trx(),
            storage.trx_origin(),
            &mut evm_backend,
            None,
        )?);
        storage.alloc_evm(evm);
        storage.alloc_executor_state(state_data);
    } else {
        let mut state_data = storage.read_executor_state();
        let mut evm = storage.read_evm();

        let evm_backend = ExecutorState::new(account_storage, &mut state_data);
        evm.reinit(&evm_backend);
    };
    Ok(())
}

fn finalize<'a, 'b>(
    steps_executed: u64,
    mut storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'a>,
    results: Option<(&'b ExitStatus, &'b Vector<Action>)>,
    mut gasometer: Gasometer,
    touched_accounts: TreeMap<Pubkey, u64>,
) -> Result<()> {
    debug_print!("finalize");

    storage.update_touched_accounts(&touched_accounts)?;
    storage.increment_steps_executed(steps_executed)?;
    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(),
        &storage.steps_executed().to_le_bytes(),
    ]);

    if steps_executed > 0 {
        accounts.transfer_treasury_payment()?;
    }

    let status = if let Some((status, actions)) = results {
        if accounts.allocate(actions)? == AllocateResult::Ready {
            accounts.apply_state_change(actions)?;
            Some(status)
        } else {
            None
        }
    } else {
        None
    };

    gasometer.record_operator_expenses(accounts.operator());

    let used_gas = gasometer.used_gas();
    let total_used_gas = gasometer.used_gas_total();
    log_data(&[
        b"GAS",
        &used_gas.to_le_bytes(),
        &total_used_gas.to_le_bytes(),
    ]);

    // Calculate priority fee for the current iteration.
    let priority_fee_in_tokens = priority_fee_txn_calculator::handle_priority_fee(
        storage.trx(),
        LAMPORTS_PER_SIGNATURE.into(),
    )?;

    storage.consume_gas(
        used_gas,
        priority_fee_in_tokens,
        accounts.db().try_operator_balance(),
    )?;

    if let Some(status) = status {
        log_return_value(&status);

        let mut origin = accounts.origin(storage.trx_origin(), storage.trx())?;
        origin.increment_revision(accounts.rent(), accounts.db())?;

        storage.refund_unused_gas(&mut origin)?;
        storage.finalize(accounts.program_id())?;
    }

    Ok(())
}

fn finalize_interrupted(
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
    gasometer: &mut Gasometer,
    state_data: &ExecutorStateData,
) -> Result<()> {
    debug_print!("finalize_interrupted");

    let chain_id = storage
        .trx()
        .chain_id()
        .unwrap_or(crate::config::DEFAULT_CHAIN_ID);
    let gas_limit = storage.trx().gas_limit();
    let gas_price = storage.trx().gas_price();

    let (exit_reason, steps_executed) = {
        let mut backend = SyncedExecutorState::new_with_state_data(account_storage, state_data);
        let mut evm = storage.read_evm::<SyncedEvmBackend, NoopEventListener>();

        let instruction =  Instruction  {
            program_id: evm.context.interrupted_instruction_program_id.clone().expect("program_id is Some"),
            accounts: evm.context.interrupted_instruction_accounts.clone().expect("accounts is Some").to_vec().into(),
            data: evm.context.interrupted_instruction_data.clone().expect("data is Some").to_vec().into(),
        };
        let signer_seeds = evm.context.interrupted_signer_seeds.clone().expect("interrupted_signer_seeds is Some");
        let lamports = evm.context.interrupted_lamports.clone().expect("interrupted_lamports is Some");

        log_msg!("finalize_interrupted:: execute_external_instruction before");
        let result = execute_external_instruction(
            &mut backend,
            &mut evm.context,
            instruction,
            signer_seeds,
            lamports,
        );
        log_msg!("finalize_interrupted:: execute_external_instruction after");
        if let Ok(return_data) = result {
            log_msg!("finalize_interrupted:: opcode_return_impl before");
            let _ = evm.opcode_return_impl(return_data, &mut backend);
            log_msg!("finalize_interrupted:: opcode_return_impl after");
        }
        log_msg!("finalize_interrupted:: evm execute before");
        let (result, steps_executed, _, _) = evm.execute(u64::MAX, &mut backend)?;
        log_msg!("finalize_interrupted:: evm execute after");
        (result, steps_executed)
    };
    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(), // Iteration steps
        &steps_executed.to_le_bytes(), // Total steps is the same as iteration steps
    ]);
    account_storage.increment_revision_for_modified_contracts()?;
    account_storage.transfer_treasury_payment()?;

    gasometer.record_operator_expenses(account_storage.operator());
    let used_gas = gasometer.used_gas();
    if used_gas > gas_limit {
        return Err(Error::OutOfGas(gas_limit, used_gas));
    }
    log_data(&[b"GAS", &used_gas.to_le_bytes(), &used_gas.to_le_bytes()]);

    let gas_cost = used_gas.saturating_mul(gas_price);
    let priority_fee = priority_fee_txn_calculator::handle_priority_fee(&storage.trx(), used_gas)?;
    account_storage.transfer_gas_payment(
        storage.trx_origin(),
        chain_id,
        gas_cost + priority_fee,
    )?;

    log_return_value(&exit_reason);
    return Ok(());
}

pub fn log_return_value(status: &ExitStatus) {
    let code: u8 = match status {
        ExitStatus::Stop => 0x11,
        ExitStatus::Return(_) => 0x12,
        ExitStatus::Suicide => 0x13,
        ExitStatus::Interrupted => 0x14,
        ExitStatus::Revert(_) => 0xd0,
        ExitStatus::StepLimit | ExitStatus::Cancel => unreachable!(),
    };

    log_msg!("exit_status={:#04X}", code); // Tests compatibility
    if let ExitStatus::Revert(msg) = status {
        crate::error::print_revert_message(msg);
    }

    log_data(&[b"RETURN", &[code]]);
}
