use crate::account::{
    BorrowedAccountInfo, Operator, OperatorBalanceAccount, OperatorBalanceValidator,
    TransactionTree,
};
use crate::debug::log_data;
use crate::error::Result;
use crate::gasometer::Gasometer;
use crate::instruction::instruction_internals::holder_parse_trx;
use crate::instruction::scheduled_transaction_skip_from_instruction::calculate_gas_for_skip;
use crate::instruction::scheduled_transaction_start::validate_scheduled_tx;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Account");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;

    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;
    let mut operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    let mut borrowed_data = accounts[0].try_borrow_mut_data()?;
    let (trx, _) = holder_parse_trx(
        BorrowedAccountInfo::new(&accounts[0], &mut borrowed_data),
        &operator,
        program_id,
        true,
    )?;
    let _ = validate_scheduled_tx(&trx, tree_index)?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&trx)?;
    let miner_address = operator_balance.miner(transaction_tree.payer());

    log_data(&[b"HASH", &trx.hash]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    transaction_tree.skip_transaction(&trx)?;

    if let Some(operator_balance) = &mut operator_balance {
        let mut gasometer = Gasometer::new(U256::ZERO, &operator)?;
        gasometer.record_write_to_holder(&trx);

        let gas = calculate_gas_for_skip(&trx, gasometer)?;

        assert_eq!(transaction_tree.chain_id(), operator_balance.chain_id());
        transaction_tree.burn(gas)?;
        operator_balance.mint(gas)?;
    }

    Ok(())
}
