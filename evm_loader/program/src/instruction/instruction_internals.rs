use std::ops::DerefMut;

use solana_program::pubkey::Pubkey;

use crate::account::{AllocateResult, BorrowedAccountInfo, Holder, Operator, StateAccount};
use crate::account_storage::{AccountStorage, ProgramAccountStorage};
use crate::allocator::acc_allocator;
use crate::debug::log_data;
use crate::error::Result;
use crate::evm::tracing::NoopEventListener;
use crate::evm::{ExitStatus, Machine};
use crate::executor::precompile_extension::call_solana::execute_external_instruction;
use crate::executor::{Action, ExecutorState, ExecutorStateData, SyncedExecutorState};
use crate::gasometer::Gasometer;
use crate::types::Vector;
use crate::types::{Transaction, TrxView};

use solana_program::instruction::Instruction;

pub type SyncedEvmBackend<'a, 'r> = SyncedExecutorState<'r, ProgramAccountStorage<'a>>;
pub type EvmBackend<'a, 'r> = ExecutorState<'r, ProgramAccountStorage<'a>>;
pub type Evm = Machine<NoopEventListener>;

pub fn allocate_evm(
    trx: &Transaction,
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
) -> Result<()> {
    storage.reset_steps_executed();

    // Dealloc evm that was potentially alloced in previous iterations before the reset.
    if storage.evm().is_some() {
        storage.evm_mut().take();
    }

    let mut state_data = storage.executor_state_mut();
    *state_data = Some(ExecutorStateData::new(account_storage));
    let mut evm_backend =
        ExecutorState::new(account_storage, state_data.deref_mut().as_mut().unwrap());
    storage.evm_mut().replace(Evm::new_from_tx(
        trx,
        storage.trx_origin(),
        &mut evm_backend,
        None,
    )?);

    Ok(())
}

pub fn reinit_evm(
    account_storage: &mut ProgramAccountStorage<'_>,
    storage: &mut StateAccount<'_>,
    reallocate: bool,
) -> Result<()> {
    if reallocate {
        storage.reset_steps_executed();

        let mut state_data = storage.executor_state_mut();
        *state_data = Some(ExecutorStateData::new(account_storage));
        let mut evm_backend =
            ExecutorState::new(account_storage, state_data.deref_mut().as_mut().unwrap());
        let evm = storage.evm_mut().take();
        storage.evm_mut().replace(Evm::new_from_machine(
            evm.unwrap(),
            storage.trx(),
            storage.trx_origin(),
            &mut evm_backend,
            None,
        )?);
    } else {
        let mut state_data = storage.executor_state_mut();
        let mut evm = storage.evm_mut();

        let evm_backend =
            ExecutorState::new(account_storage, state_data.deref_mut().as_mut().unwrap());
        evm.as_mut().unwrap().reinit(&evm_backend);
    };
    Ok(())
}

pub fn holder_parse_trx(
    info: BorrowedAccountInfo<'_>,
    operator: &Operator,
    program_id: &Pubkey,
    is_scheduled: bool,
) -> Result<(Transaction, Vec<u8>)> {
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

    Ok((trx, transaction_rlp_copy))
}

pub fn finalize<'a, 'b: 'a, 'c>(
    steps_executed: u64,
    mut storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'b>,
    mut gasometer: Gasometer,
    apply_state: bool,
    provided_execution_result: Option<(&ExitStatus, &Vector<Action>)>,
) -> Result<()> {
    debug_print!("finalize");

    storage.update_touched_accounts(accounts.program_id(), accounts.db())?;
    storage.increment_steps_executed(steps_executed)?;
    storage.publish_block_params();
    log_data(&[
        b"STEPS",
        &steps_executed.to_le_bytes(),
        &storage.steps_executed().to_le_bytes(),
    ]);

    if accounts.has_treasury() {
        accounts.transfer_treasury_payment()?;
    }

    if {
        let root = storage.root_ref_mut();
        let mut executor_state = root.executor_state.borrow_mut();

        let (storage_header, status) = {
            let (execution_result, _, timestamped_contracts) =
                executor_state.as_mut().unwrap().deconstruct();
            let status =
                if let Some((status, actions)) = provided_execution_result.or(execution_result) {
                    if apply_state && accounts.allocate(actions)? == AllocateResult::Ready {
                        accounts.apply_state_change(actions)?;
                        accounts.update_timestamped_contracts(timestamped_contracts.keys())?;
                        Some(status)
                    } else {
                        None
                    }
                } else {
                    None
                };
            (&mut root.plain_data, status)
        };

        gasometer.record_solana_transaction_cost(storage_header)?;
        gasometer.record_operator_expenses(accounts.operator());

        let used_gas = gasometer.used_gas();
        let total_used_gas = gasometer.used_gas_total();
        log_data(&[
            b"GAS",
            &used_gas.to_le_bytes(),
            &total_used_gas.to_le_bytes(),
        ]);

        storage_header.consume_gas(used_gas, accounts.db().try_operator_balance())?;

        if let Some(status) = status {
            log_return_value(&status);

            // refund gas for scheduled transaction is happening in transaction_finish.
            if !storage_header.is_scheduled_tx() {
                let mut origin = accounts.origin(storage_header.origin, storage_header)?;
                origin.increment_revision(accounts.rent(), accounts.db())?;

                storage_header.refund_unused_gas(&mut origin)?;
            }

            true
        } else {
            false
        }
    } {
        storage.finalize(accounts.program_id())?;
    }

    Ok(())
}

pub fn finalize_interrupted<'a, 'b: 'a>(
    storage: StateAccount<'a>,
    mut accounts: ProgramAccountStorage<'b>,
    gasometer: Gasometer,
) -> Result<()> {
    debug_print!("finalize_interrupted");

    let (exit_reason, steps_executed, _, _) = {
        let mut state_ref = storage.executor_state_mut();
        let state_data = state_ref.as_mut().unwrap();
        accounts.apply_state_change(state_data.into_actions())?;
        let mut backend = SyncedExecutorState::new_with_state_data(&mut accounts, state_data);

        let mut evm_ref = storage.evm_mut();
        let evm = evm_ref.as_mut().unwrap();
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

    let no_actions = Vector::new_in(acc_allocator());
    finalize(
        steps_executed,
        storage,
        accounts,
        gasometer,
        true,
        Some((&exit_reason, &no_actions)),
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
