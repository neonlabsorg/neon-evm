use solana_program::pubkey::Pubkey;

use crate::account::{AccountsDB, AllocateResult, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::config::{EVM_STEPS_LAST_ITERATION_MAX, EVM_STEPS_MIN};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::{ExitStatus, Machine};
use crate::executor::{Action, ExecutorState, ExecutorStateData};
use crate::gasometer::{Gasometer, LAMPORTS_PER_SIGNATURE};
use crate::types::boxx::boxx;
use crate::types::TransactionPayload;
use crate::types::TreeMap;
use crate::types::Vector;

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

    // Burn `gas_limit` tokens from the origin account
    // Later we will mint them to the operator
    // Remaining tokens are returned to the origin in the last iteration
    let gas_limit_in_tokens = storage.trx().gas_limit_in_tokens()?;
    origin_account.burn(gas_limit_in_tokens)?;

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
    gasometer: Gasometer,
    reset: bool,
) -> Result<()> {
    debug_print!("do_continue");

    if (step_count < EVM_STEPS_MIN) && (storage.trx().gas_price() > 0) {
        return Err(Error::Custom(format!(
            "Step limit {step_count} below minimum {EVM_STEPS_MIN}"
        )));
    }

    let mut account_storage = ProgramAccountStorage::new(accounts)?;
    if reset {
        log_data(&[b"RESET"]);
    }

    allocate_or_reinit_state(&mut account_storage, &mut storage, reset)?;
    let mut state_data = storage.read_executor_state();
    let mut evm = storage.read_evm::<EvmBackend, NoopEventListener>();
    let mut backend = ExecutorState::new(&mut account_storage, &mut state_data);

    let mut steps_executed = 0;
    if backend.exit_status().is_none() {
        let (exit_status, steps_returned, _) = evm.execute(step_count, &mut backend)?;
        if exit_status != ExitStatus::StepLimit {
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

        // Dealloc objects that were potentially alloced in previous iterations before the reset.
        if storage.is_evm_alloced() {
            storage.dealloc_evm::<EvmBackend, NoopEventListener>();
        }
        if storage.is_executor_state_alloced() {
            storage.dealloc_executor_state();
        }

        let mut state_data = boxx(ExecutorStateData::new(account_storage));
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

    storage.consume_gas(used_gas, accounts.db().try_operator_balance())?;

    // In case the transaction is DynamicFee:
    // (1) validate that the Operator specified the priority fee according to transaction.
    // (2) charge the User in favor of Operator with amount of `priority_fee_per_gas` * `LAMPORTS_PER_SIGNATURE`.
    if let TransactionPayload::DynamicFee(dynamic_fee_payload) = &storage.trx().transaction {
        // Validate.
        accounts.db().sysvar().validate_priority_fee(
            dynamic_fee_payload.max_priority_fee_per_gas,
            dynamic_fee_payload.max_fee_per_gas,
        )?;

        let priority_fee_in_tokens = dynamic_fee_payload
            .max_priority_fee_per_gas
            .checked_mul(LAMPORTS_PER_SIGNATURE.into())
            .ok_or(Error::PriorityFeeError(
                "max_priority_fee_per_gas * LAMPORTS_PER_SIGNATURE overflow".to_string(),
            ))?;

        // Transfer priority fee.
        accounts.transfer_gas_payment(
            storage.trx_origin(),
            storage
                .trx()
                .chain_id()
                .unwrap_or(accounts.default_chain_id()),
            priority_fee_in_tokens,
        )?;
        log_data(&[b"PRIORITYFEE", &priority_fee_in_tokens.to_le_bytes()]);
    }

    if let Some(status) = status {
        log_return_value(&status);

        let mut origin = accounts.origin(storage.trx_origin(), storage.trx())?;
        origin.increment_revision(accounts.rent(), accounts.db())?;

        storage.refund_unused_gas(&mut origin)?;
        storage.finalize(accounts.program_id())?;
    }

    Ok(())
}

pub fn log_return_value(status: &ExitStatus) {
    let code: u8 = match status {
        ExitStatus::Stop => 0x11,
        ExitStatus::Return(_) => 0x12,
        ExitStatus::Suicide => 0x13,
        ExitStatus::Revert(_) => 0xd0,
        ExitStatus::StepLimit => unreachable!(),
    };

    log_msg!("exit_status={:#04X}", code); // Tests compatibility
    if let ExitStatus::Revert(msg) = status {
        crate::error::print_revert_message(msg);
    }

    log_data(&[b"RETURN", &[code]]);
}
