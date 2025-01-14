use super::priority_fee_txn_calculator::handle_priority_fee;
use crate::account::{
    Holder, Operator, OperatorBalanceAccount, OperatorBalanceValidator, TransactionTree,
};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::scheduled_transaction_start::validate_scheduled_tx;
use crate::types::Transaction;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn calculate_gas_for_skip(trx: &Transaction, gasometer: &Gasometer) -> Result<U256> {
    let gas_limit = trx.gas_limit();
    let gas_price = trx.gas_price();

    let used_gas = gasometer.used_gas();
    if used_gas > gas_limit {
        return Err(Error::OutOfGas(gas_limit, used_gas));
    }

    log_data(&[b"GAS", &used_gas.to_le_bytes(), &used_gas.to_le_bytes()]);

    let gas_cost = used_gas.saturating_mul(gas_price);
    let priority_fee = handle_priority_fee(&trx)?;

    let gas = gas_cost.saturating_add(priority_fee);
    Ok(gas)
}

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Instruction");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;
    let message = &instruction[4..];

    let mut holder = Holder::from_account(program_id, accounts[0].clone())?;
    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;
    let mut operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    holder.validate_owner(&operator)?;
    holder.init_heap(0)?;

    let trx = Transaction::scheduled_from_rlp(message)?;
    let _ = validate_scheduled_tx(&trx, tree_index)?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&trx)?;
    let miner_address = operator_balance.miner(transaction_tree.payer());

    log_data(&[b"HASH", &trx.hash]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    transaction_tree.skip_transaction(&trx)?;

    if let Some(operator_balance) = &mut operator_balance {
        let mut gasometer = Gasometer::new(U256::ZERO, &operator)?;
        gasometer.record_solana_transaction_cost();

        let gas = calculate_gas_for_skip(&trx, &gasometer)?;

        assert_eq!(transaction_tree.chain_id(), operator_balance.chain_id());
        transaction_tree.burn(gas)?;
        operator_balance.mint(gas)?;
    }

    Ok(())
}
