use crate::account::{
    AccountDispatch, Holder, Operator, OperatorBalance, StateAccount, Treasury, TAG_HOLDER,
    TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::error::{Error, Result};
use crate::platform::Solana;
use crate::transaction_process::transaction_step;

use arrayref::array_ref;
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
    let step_limit = u64::from(u32::from_le_bytes(*array_ref![instruction, 4, 4]));

    let holder_info = accounts[0].clone();
    let operator = Operator::from_account_info(&accounts[1])?;
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    let mut solana = Solana::new(&accounts[1..], operator.clone(), operator_balance)?;
    solana.pay_to_treasury(treasury)?;

    match holder_info.tag(program_id)? {
        TAG_HOLDER => {
            let holder = Holder::from_account(program_id, holder_info)?;
            holder.validate(&operator)?;

            let rlp = holder.transaction()?;
            let owner = holder.owner();
            let account = holder.into_account();

            solana.use_gasometer(|g| g.record_write_to_holder(&rlp));

            let mut state = StateAccount::new(account, owner, rlp, &mut solana)?;
            if increase_gas_limit {
                let mut root = state.root_mut();
                root.increase_gas_limit_for_transactions_without_chain_id(&mut solana)?;
            }

            transaction_step::start_iterative(solana, state)
        }
        TAG_STATE => {
            let state = StateAccount::restore(holder_info, &mut solana)?;
            transaction_step::execute_iterative(solana, state, step_limit)
        }
        TAG_SCHEDULED_STATE_CANCELLED | TAG_SCHEDULED_STATE_FINALIZED => {
            Err(Error::ScheduledTxAlreadyComplete(holder_info.pubkey()))
        }
        TAG_STATE_FINALIZED => Err(Error::StorageAccountFinalized),
        _ => Err(Error::AccountInvalidTag(holder_info.pubkey(), TAG_HOLDER)),
    }?;

    Ok(())
}
