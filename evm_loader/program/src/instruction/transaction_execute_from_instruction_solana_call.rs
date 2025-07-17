use crate::account::{Holder, Operator, OperatorBalance, Treasury};
use crate::debug::log_data;
use crate::error::Result;
use crate::platform::Solana;
use crate::transaction_process::transaction_execute;
use crate::types::EncodedTransaction;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Execute Ethereum transaction in a single Solana transaction
pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Execute Transaction from Instruction with Solana call");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let messsage = &instruction[4..];

    let holder = Holder::from_account_info(program_id, &accounts[0])?;
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1])? };
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    holder.validate(&operator)?;
    let allocator = holder.into_allocator();

    let encoded_tansaction = EncodedTransaction::from_rlp(messsage);

    let trx = encoded_tansaction.decode()?;
    let origin = trx.recover_caller_address()?;

    let mut solana = Solana::new_with_solana_call(accounts, operator, operator_balance)?;

    log_data(&[b"HASH", &trx.hash()]);
    solana.log_miner_address(origin);

    solana.pay_to_treasury(treasury)?;
    transaction_execute::execute_with_solana_call(solana, trx, origin, allocator)
}
