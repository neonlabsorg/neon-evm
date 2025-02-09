use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::{AllocateResult, Holder, Operator, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::allocator::acc_allocator;
use crate::debug::log_data;
use crate::error::Result;
use crate::evm::tracing::NoopEventListener;
use crate::evm::{ExitStatus, Machine};
use crate::executor::precompile_extension::call_solana::execute_external_instruction;
use crate::executor::{Action, ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::gasometer::Gasometer;
use crate::instruction::priority_fee_txn_calculator;
use crate::types::boxx::boxx;
use crate::types::{Address, Vector};
use crate::types::{Transaction, TreeMap};

use solana_program::instruction::Instruction;

pub type SyncedEvmBackend<'a, 'r> = SyncedExecutorState<'r, ProgramAccountStorage<'a>>;
pub type EvmBackend<'a, 'r> = ExecutorState<'r, ProgramAccountStorage<'a>>;
pub type Evm<'a, 'r> = Machine<EvmBackend<'a, 'r>, NoopEventListener>;

pub fn allocate_evm(
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
) -> Result<()> {
    storage.reset_steps_executed();

    // Dealloc evm that was potentially alloced in previous iterations before the reset.
    if storage.is_evm_alloced() {
        storage.dealloc_evm::<EvmBackend, NoopEventListener>();
    }

    // Dealloc executor state that was potentially alloced in previous iterations before the reset.
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

    Ok(())
}

pub fn reinit_evm(
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
    reallocate: bool,
) -> Result<()> {
    if reallocate {
        allocate_evm(account_storage, storage)?;
    } else {
        let mut state_data = storage.read_executor_state();
        let mut evm = storage.read_evm();

        let evm_backend = ExecutorState::new(account_storage, &mut state_data);
        evm.reinit(&evm_backend);
    };
    Ok(())
}

pub fn holder_parse_trx(
    info: AccountInfo<'_>,
    operator: &Operator,
    program_id: &Pubkey,
    is_scheduled: bool,
) -> Result<Transaction> {
    let mut holder = Holder::from_account(program_id, info)?;

    // We have to initialize the heap before creating Transaction object, but since
    // transaction's rlp itself is stored in the holder account, we have two options:
    // 1. Copy the rlp and initialize the heap right after the holder's header.
    //   This way, the space occupied by the rlp within holder will be reused.
    // 2. Don't copy the rlp, initialize the heap after transaction rlp in the holder.
    // The first option (chosen) saves the holder space in exchange for compute units.
    // The second option wastes the holder space (because transaction bytes will be
    // stored two times), but doesnt copy.
    let transaction_rlp_copy = holder.transaction().to_vec();
    holder.init_heap(0)?;
    holder.validate_owner(&operator)?;

    let trx = {
        if is_scheduled {
            Transaction::scheduled_from_rlp(&transaction_rlp_copy)
        } else {
            Transaction::from_rlp(&transaction_rlp_copy)
        }
    }?;

    holder.validate_transaction(&trx)?;

    Ok(trx)
}

pub fn finalize<'a, 'b>(
    steps_executed: u64,
    mut storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'a>,
    results: Option<(&'b ExitStatus, &'b Vector<Action>)>,
    mut gasometer: Gasometer,
    touched_accounts: TreeMap<Pubkey, u64>,
    timestamped_contracts: TreeMap<Address, ()>,
) -> Result<()> {
    debug_print!("finalize");

    storage.update_touched_accounts(accounts.program_id(), accounts.db(), &touched_accounts)?;
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
            accounts.update_timestamped_contracts(timestamped_contracts.keys())?;
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
    let trx = storage.trx();
    let priority_fee_in_tokens = if status.is_some() {
        let total_priority_fee_used = storage.priority_fee_in_tokens_used();
        priority_fee_txn_calculator::finalize_priority_fee(
            trx,
            total_used_gas,
            total_priority_fee_used,
        )?
    } else {
        priority_fee_txn_calculator::handle_priority_fee(trx)?
    };

    storage.consume_gas(
        used_gas,
        priority_fee_in_tokens,
        accounts.db().try_operator_balance(),
    )?;

    if let Some(status) = status {
        log_return_value(&status);

        let trx = storage.trx();
        // refund gas for scheduled transaction is happening in transaction_finish.
        if !trx.is_scheduled_tx() {
            let mut origin = accounts.origin(storage.trx_origin(), trx)?;
            origin.increment_revision(accounts.rent(), accounts.db())?;

            storage.refund_unused_gas(&mut origin)?;
        }

        storage.finalize(accounts.program_id())?;
    }

    Ok(())
}

pub fn finalize_interrupted<'a>(
    storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'a>,
    gasometer: Gasometer,
    state_data: &mut ExecutorStateData,
) -> Result<()> {
    debug_print!("finalize_interrupted");

    accounts.apply_state_change(state_data.into_actions())?;
    let (exit_reason, steps_executed, _, _) = {
        let mut backend = SyncedExecutorState::new_with_state_data(&mut accounts, state_data);
        let mut evm = storage.read_evm::<SyncedEvmBackend, NoopEventListener>();
        let interrupted_state = storage
            .interrupted_state()
            .expect("storage.interrupted_state should be Some within finalize_interrupted context");

        let result = execute_external_instruction(
            &mut backend,
            evm.context(),
            Instruction {
                program_id: interrupted_state.instruction.program_id,
                accounts: interrupted_state.instruction.accounts.to_vec(),
                data: interrupted_state.instruction.data.to_vec(),
            },
            interrupted_state.signer_seeds.clone(),
            interrupted_state.lamports,
        );
        if let Ok(return_data) = result {
            evm.opcode_return_impl(return_data, &mut backend)?;
            evm.increment_pc();
        }
        evm.execute(u64::MAX, &mut backend)?
    };
    let (_, touched_accounts, timestamped_contracts) = state_data.deconstruct();
    let no_actions = Vector::new_in(acc_allocator());
    finalize(
        steps_executed,
        storage,
        accounts,
        Some((&exit_reason, &no_actions)),
        gasometer,
        touched_accounts,
        timestamped_contracts,
    )
}

pub fn log_return_value(status: &ExitStatus) {
    let code: u8 = match status {
        ExitStatus::Stop => 0x11,
        ExitStatus::Return(_) => 0x12,
        ExitStatus::Suicide => 0x13,
        ExitStatus::Interrupted(_) => 0x14,
        ExitStatus::Revert(_) => 0xd0,
        ExitStatus::StepLimit | ExitStatus::Cancel => unreachable!(),
    };

    log_msg!("exit_status={:#04X}", code); // Tests compatibility
    if let ExitStatus::Revert(msg) = status {
        crate::error::print_revert_message(msg);
    }

    log_data(&[b"RETURN", &[code]]);
}
