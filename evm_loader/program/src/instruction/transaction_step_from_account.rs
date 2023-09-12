use crate::account::{
    program, AccountsDB, BalanceAccount, Holder, Operator, StateAccount, Treasury, TAG_HOLDER,
    TAG_STATE, TAG_STATE_FINALIZED,
};

use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::transaction_step::{do_begin, do_continue};
use crate::types::Transaction;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    solana_program::msg!("Instruction: Begin or Continue Transaction from Account");

    process_inner(program_id, accounts, instruction, false)
}

pub fn process_inner<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
    increase_gas_limit: bool,
) -> Result<()> {
    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let step_count = u64::from(u32::from_le_bytes(*array_ref![instruction, 4, 4]));

    let holder_or_storage = &accounts[0];

    let operator = Operator::from_account(&accounts[1])?;
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let operator_balance = BalanceAccount::from_account(program_id, accounts[3].clone(), None)?;
    let system = program::System::from_account(&accounts[4])?;

    let accounts_db = AccountsDB::new(
        &accounts[5..],
        operator,
        Some(operator_balance),
        Some(system),
        Some(treasury),
    );

    match crate::account::tag(program_id, holder_or_storage)? {
        TAG_HOLDER => {
            let trx = {
                let holder = Holder::from_account(program_id, holder_or_storage.clone())?;
                holder.validate_owner(accounts_db.operator())?;

                let message = holder.transaction();
                let trx = Transaction::from_rlp(&message)?;

                holder.validate_transaction(&trx)?;

                trx
            };

            solana_program::log::sol_log_data(&[b"HASH", &trx.hash]);

            let origin = trx.recover_caller_address()?;
            let mut storage = StateAccount::new(
                program_id,
                holder_or_storage.clone(),
                &accounts_db,
                origin,
                &trx,
            )?;

            if increase_gas_limit {
                assert!(trx.chain_id().is_none());
                storage.use_gas_limit_multiplier();
            }

            let mut gasometer = Gasometer::new(U256::ZERO, accounts_db.operator())?;
            gasometer.record_solana_transaction_cost();
            gasometer.record_address_lookup_table(accounts);
            gasometer.record_iterative_overhead();
            gasometer.record_write_to_holder(&trx);

            do_begin(accounts_db, storage, gasometer, trx, origin)
        }
        TAG_STATE => {
            let storage =
                StateAccount::restore(program_id, holder_or_storage.clone(), &accounts_db, false)?;

            solana_program::log::sol_log_data(&[b"HASH", &storage.trx_hash()]);

            let mut gasometer = Gasometer::new(storage.gas_used(), accounts_db.operator())?;
            gasometer.record_solana_transaction_cost();

            do_continue(step_count, accounts_db, storage, gasometer)
        }
        TAG_STATE_FINALIZED => Err(Error::StorageAccountFinalized),
        _ => Err(Error::AccountInvalidTag(*holder_or_storage.key, TAG_HOLDER)),
    }
}
