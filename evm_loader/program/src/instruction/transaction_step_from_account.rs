use crate::account::{
    program, AccountDispatch, AccountsDB, AccountsStatus, Operator, OperatorBalance,
    OperatorBalanceValidator, StateAccount, Treasury, TAG_HOLDER, TAG_SCHEDULED_STATE_CANCELLED,
    TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::holder_parse_trx;
use crate::instruction::transaction_step::{do_begin, do_continue};
use crate::types::TrxView;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Begin or Continue Transaction from Account");

    process_inner(program_id, accounts, instruction, false)
}

pub fn process_inner(
    program_id: Pubkey,
    accounts: &[AccountInfo],
    instruction: &[u8],
    increase_gas_limit: bool,
) -> Result<()> {
    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let step_count = u64::from(u32::from_le_bytes(*array_ref![instruction, 4, 4]));

    let holder_or_storage = accounts[0].clone();

    let operator = Operator::from_account_info(&accounts[1])?;
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;
    let system = program::System::from_account_info(&accounts[4])?;

    operator_balance.validate_owner(&operator)?;

    let accounts_db = AccountsDB::new(
        &accounts[5..],
        operator.clone(),
        operator_balance.clone(),
        Some(system),
        Some(treasury),
    );

    match holder_or_storage.tag(program_id)? {
        TAG_HOLDER => {
            let (mut trx, tx_rlp) =
                holder_parse_trx(&holder_or_storage, &operator, program_id, false)?;
            let origin = trx.recover_caller_address()?;

            operator_balance.validate_transaction(&trx)?;
            let miner_address = operator_balance.miner(origin);

            log_data(&[b"HASH", &trx.hash]);
            log_data(&[b"MINER", miner_address.as_bytes()]);

            if increase_gas_limit {
                assert!(trx.chain_id().is_none());
                trx.use_gas_limit_multiplier();
            }

            let mut gasometer = Gasometer::new(U256::ZERO, &operator)?;
            gasometer.record_address_lookup_table(accounts);
            gasometer.record_write_to_holder(&trx);

            let storage = StateAccount::new(
                program_id,
                &holder_or_storage,
                &accounts_db,
                origin,
                &trx,
                tx_rlp.as_slice(),
                None,
            )?;

            do_begin(trx, accounts_db, storage, gasometer)
        }
        TAG_STATE => {
            let (storage, accounts_status, parsed_trx) =
                StateAccount::restore(program_id, &holder_or_storage, &accounts_db)?;

            operator_balance.validate_transaction(storage.trx())?;
            let miner_address = operator_balance.miner(storage.trx_origin());

            log_data(&[b"HASH", &storage.trx().hash()]);
            log_data(&[b"MINER", miner_address.as_bytes()]);

            let gasometer = Gasometer::new(storage.gas_used(), &operator)?;

            let reset = accounts_status != AccountsStatus::Ok;
            do_continue(
                step_count,
                accounts_db,
                storage,
                gasometer,
                reset,
                parsed_trx,
            )
        }
        TAG_SCHEDULED_STATE_CANCELLED | TAG_SCHEDULED_STATE_FINALIZED => {
            Err(Error::ScheduledTxAlreadyComplete(*holder_or_storage.key))
        }
        TAG_STATE_FINALIZED => Err(Error::StorageAccountFinalized),
        _ => Err(Error::AccountInvalidTag(*holder_or_storage.key, TAG_HOLDER)),
    }?;

    Ok(())
}
