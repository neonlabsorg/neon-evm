use crate::account::{AccountsDB, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::config::{EVM_STEPS_LAST_ITERATION_MAX, EVM_STEPS_MIN};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::ExitStatus;
use crate::executor::ExecutorState;
use crate::gasometer::Gasometer;
use crate::instruction::transaction_internals::{allocate_evm, finalize, reinit_evm, EvmBackend};

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

    reinit_evm(&mut account_storage, &mut storage, reset)?;
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
