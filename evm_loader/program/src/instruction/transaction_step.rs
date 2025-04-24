use std::cell::RefMut;

use crate::account::{AccountsDB, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::config::{EVM_STEPS_LAST_ITERATION_MAX, EVM_STEPS_MIN};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::ExitStatus;
use crate::executor::ExecutorState;
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::{
    allocate_evm, finalize, finalize_interrupted, reinit_evm,
};
use crate::types::{Transaction, TrxView};

pub fn do_begin<'b, 'a: 'b>(
    tx: Transaction,
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'b>,
    gasometer: Gasometer,
) -> Result<()> {
    debug_print!("do_begin");

    let mut account_storage = ProgramAccountStorage::new(accounts)?;

    let origin = storage.trx_origin();

    storage.trx().validate(origin, &account_storage, None)?;

    // Increment origin nonce in the first iteration
    // This allows us to run multiple iterative transactions from the same sender in parallel
    // These transactions are guaranteed to start in a correct sequence
    // BUT they finalize in an undefined order
    let mut origin_account = account_storage.origin(origin, storage.trx())?;
    origin_account.increment_revision(account_storage.rent(), account_storage.db())?;
    origin_account.increment_nonce()?;

    // Burn `gas_limit` tokens from the origin account.
    // Later we will mint them to the operator.
    // Remaining tokens are returned to the origin in the last iteration.
    let gas_limit_in_tokens = storage.trx().gas_limit_in_tokens()?;
    origin_account.burn(gas_limit_in_tokens)?;

    // TODO for scheduled transactions, evm should be created with origin:=payer.
    allocate_evm(&tx, &mut account_storage, &mut storage)?;

    finalize(0, storage, account_storage, gasometer, true, None)
}

pub fn do_continue<'b, 'a: 'b>(
    step_count: u64,
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'b>,
    gasometer: Gasometer,
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
    reinit_evm(&mut account_storage, &mut storage, reset)?;

    if storage.interrupted_state().is_some() {
        return finalize_interrupted(storage, account_storage, gasometer);
    }

    let steps_executed = {
        let root = storage.root_ref_mut();
        let mut state_data = RefMut::map(root.executor_state.borrow_mut(), |opt| {
            opt.as_mut().unwrap()
        });
        let mut evm = RefMut::map(root.machine_state.borrow_mut(), |opt| opt.as_mut().unwrap());

        let mut backend = ExecutorState::new(&mut account_storage, &mut state_data);
        let mut steps_executed = 0;

        if backend.exit_status().is_none() {
            let (exit_status, steps_returned, _, _) = evm.execute(step_count, &mut backend)?;

            if let ExitStatus::Interrupted(state) = exit_status {
                root.interrupted_state = *state;
            } else if ExitStatus::StepLimit != exit_status {
                backend.set_exit_status(exit_status);
            }
            steps_executed = steps_returned;
        }

        steps_executed
    };

    finalize(
        steps_executed,
        storage,
        account_storage,
        gasometer,
        steps_executed > EVM_STEPS_LAST_ITERATION_MAX,
        None,
    )
}
