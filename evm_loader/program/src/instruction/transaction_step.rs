use crate::account::{AccountsDB, AllocateResult, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::config::{EVM_STEPS_LAST_ITERATION_MAX, EVM_STEPS_MIN};
use crate::error::{Error, Result};
use crate::evm::tracing::NoopEventListener;
use crate::evm::{ExitStatus, Machine};
use crate::executor::{Action, ExecutorState};
use crate::gasometer::Gasometer;
use crate::persistent_state::PersistentState;
use crate::types::{Address, Transaction, Vector};

type EvmBackend<'a, 'r> = ExecutorState<'r, ProgramAccountStorage<'a>>;
type Evm<'a, 'r> = Machine<EvmBackend<'a, 'r>, NoopEventListener>;

pub fn do_begin<'a>(
    accounts: AccountsDB<'a>,
    storage: StateAccount<'a>,
    gasometer: Gasometer,
    trx: Transaction,
    origin: Address,
) -> Result<()> {
    debug_print!("do_begin");

    let accounts = ProgramAccountStorage::new(accounts)?;
    // create and load the PersistentState struct into the holder account heap. 
    PersistentState::alloc(trx, origin, &accounts);

    // Burn `gas_limit` tokens from the origin account
    // Later we will mint them to the operator
    let mut origin_balance = accounts.create_balance_account(origin, storage.trx_chain_id())?;
    origin_balance.burn(storage.gas_limit_in_tokens()?)?;

    finalize(0, storage, accounts, None, gasometer)
}

pub fn do_continue<'a>(
    step_count: u64,
    accounts: AccountsDB<'a>,
    storage: StateAccount<'a>,
    gasometer: Gasometer,
) -> Result<()> {
    debug_print!("do_continue");

    if (step_count < EVM_STEPS_MIN) && (storage.trx_gas_price() > 0) {
        return Err(Error::Custom(format!(
            "Step limit {step_count} below minimum {EVM_STEPS_MIN}"
        )));
    }

    let account_storage = ProgramAccountStorage::new(accounts)?;
    let mut persistent_state  = PersistentState::restore(&account_storage);

    let (result, steps_executed, _) = {
        match persistent_state.backend.exit_status() {
            Some(status) => (status.clone(), 0_u64, None),
            None => {
                // TODO: BORROWCHECKER CURSES HERE (FOR A REASON).
                // I NEED TO FIGURE OUT THE WAY TO AVOID IT WITHOUT MAJOR CHANGES.
                let be = &mut persistent_state.backend;
                persistent_state.root_evm.execute(step_count, be)?
            },
        }
    };

    if (result != ExitStatus::StepLimit) && (steps_executed > 0) {
        persistent_state.backend.set_exit_status(result.clone());
    }

    let results = match result {
        ExitStatus::StepLimit => None,
        _ if steps_executed > EVM_STEPS_LAST_ITERATION_MAX => None,
        result => Some((result, persistent_state.backend.into_actions())),
    };

    finalize(steps_executed, storage, account_storage, results, gasometer)
}

fn finalize<'a>(
    steps_executed: u64,
    mut storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'a>,
    results: Option<(ExitStatus, Vector<Action>)>,
    mut gasometer: Gasometer,
) -> Result<()> {
    debug_print!("finalize");

    if steps_executed > 0 {
        accounts.transfer_treasury_payment()?;
    }

    let status = if let Some((status, actions)) = results {
        if accounts.allocate(&actions)? == AllocateResult::Ready {
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
    solana_program::log::sol_log_data(&[
        b"GAS",
        &used_gas.to_le_bytes(),
        &total_used_gas.to_le_bytes(),
    ]);

    storage.consume_gas(used_gas, accounts.operator_balance())?;

    if let Some(status) = status {
        log_return_value(&status);

        let mut origin = accounts.balance_account(storage.trx_origin(), storage.trx_chain_id())?;
        storage.refund_unused_gas(&mut origin)?;

        storage.finalize(accounts.program_id(), accounts.db())?;
    }

    Ok(())
}

pub fn log_return_value(status: &ExitStatus) {
    use solana_program::log::sol_log_data;

    let code: u8 = match status {
        ExitStatus::Stop => 0x11,
        ExitStatus::Return(_) => 0x12,
        ExitStatus::Suicide => 0x13,
        ExitStatus::Revert(_) => 0xd0,
        ExitStatus::StepLimit => unreachable!(),
    };

    solana_program::msg!("exit_status={:#04X}", code); // Tests compatibility
    if let ExitStatus::Revert(msg) = status {
        crate::error::print_revert_message(msg);
    }

    sol_log_data(&[b"RETURN", &[code]]);
}
