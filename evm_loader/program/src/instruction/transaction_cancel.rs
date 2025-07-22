use crate::account::{Operator, OperatorBalance, StateAccount};
use crate::debug::log_data;
use crate::error::{Error, Result};

use crate::platform::Solana;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Cancel Transaction");

    let transaction_hash = array_ref![instruction, 0, 32];

    let state = StateAccount::from_account(program_id, accounts[0].clone())?;
    let operator = Operator::from_account_info(&accounts[1])?;
    let operator_balance = OperatorBalance::from_account_info(program_id, &accounts[2])?;

    log_data(&[b"HASH", transaction_hash]);
    log_data(&[b"MINER", operator_balance.address().as_bytes()]);

    let solana = Solana::new(&accounts[1..], operator, Some(operator_balance))?;

    execute(solana, state, *transaction_hash)
}

fn execute<'a>(
    mut solana: Solana<'a>,
    mut state: StateAccount<'a>,
    transaction_hash: [u8; 32],
) -> Result<()> {
    let stored_hash = state.transaction_hash().to_bytes();
    if stored_hash != transaction_hash {
        return Err(Error::HolderInvalidHash(stored_hash, transaction_hash));
    }

    // `root` is accessible, but we can't use any vectors inside of it
    // because the heap layout could change between transactions
    let mut root = state.root_mut();

    // Try to reward the operator for the canceled transaction to the best effort. Ignore any potential errors.
    // Releasing the Holder account is more important than ensuring the operator's profit
    solana.use_gasometer(|g| g.record_cancel_gas(&root))?;
    let _ = solana.reward_operator_from_holder(&mut root);

    // Do not refund unused gas for the scheduled transaction - it happens in the `scheduled_transaction_finish`.
    if !root.is_scheduled_transaction() {
        root.refund_unused_gas_to_origin(&mut solana)?;
    }

    std::mem::drop(root);
    state.cancel()
}
