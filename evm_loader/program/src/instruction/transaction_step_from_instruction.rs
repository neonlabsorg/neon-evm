use crate::account::{
    AccountRead, Holder, Operator, OperatorBalance, StateAccount, StateFinalizedAccount, Treasury,
    TAG_HOLDER, TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE,
    TAG_STATE_FINALIZED,
};
use crate::error::{Error, Result};
use crate::platform::Solana;
use crate::transaction_process::transaction_step;
use crate::types::EncodedTransaction;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn validate_holder<'a>(
    program_id: &Pubkey,
    info: AccountInfo<'a>,
    operator: &Operator,
    rlp: &EncodedTransaction,
) -> Result<(AccountInfo<'a>, Pubkey)> {
    match info.tag(program_id)? {
        TAG_HOLDER => {
            let holder = Holder::from_account(program_id, info)?;
            holder.validate(operator)?;

            let owner = holder.owner();
            Ok((holder.into_account(), owner))
        }
        TAG_STATE_FINALIZED => {
            let finalized = StateFinalizedAccount::from_account(program_id, info)?;
            finalized.validate_owner(operator)?;
            finalized.validate_trx(rlp)?;

            let owner = finalized.owner();
            Ok((finalized.into_account(), owner))
        }
        tag => return Err(Error::StorageAccountInvalidTag(info.pubkey(), tag)),
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Begin or Continue Transaction from Instruction");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let step_limit = u64::from(u32::from_le_bytes(*array_ref![instruction, 4, 4]));
    // skip let unique_index = u32::from_le_bytes(*array_ref![instruction, 8, 4]);
    let message = &instruction[4 + 4 + 4..];

    let holder_info = accounts[0].clone();
    let operator = Operator::from_account_info(&accounts[1])?;
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    let mut solana = Solana::new(&accounts[1..], operator.clone(), operator_balance)?;
    solana.pay_to_treasury(treasury)?;

    match holder_info.tag(program_id)? {
        TAG_HOLDER | TAG_STATE_FINALIZED => {
            let rlp = EncodedTransaction::from_rlp(message);

            let (state_info, owner) = validate_holder(program_id, holder_info, &operator, &rlp)?;
            let state = StateAccount::new(state_info, owner, rlp, &mut solana)?;

            transaction_step::start_iterative(solana, state)
        }
        TAG_STATE => {
            let state = StateAccount::restore(holder_info, &mut solana)?;

            transaction_step::execute_iterative(solana, state, step_limit)
        }
        TAG_SCHEDULED_STATE_CANCELLED | TAG_SCHEDULED_STATE_FINALIZED => {
            Err(Error::ScheduledTxAlreadyComplete(holder_info.pubkey()))
        }
        _ => Err(Error::AccountInvalidTag(holder_info.pubkey(), TAG_HOLDER)),
    }
}
