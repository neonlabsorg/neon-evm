use crate::account::{program, AccountsDB, BalanceAccount, Holder, Operator, Treasury};
use crate::debug::log_data;
use crate::error::Result;
use crate::gasometer::Gasometer;
use crate::types::{boxx::boxx, Transaction};
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Execute Ethereum transaction in a single Solana transaction
pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Execute Transaction from Account");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let mut holder = Holder::from_account(program_id, accounts[0].clone())?;

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1])? };
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let operator_balance = BalanceAccount::from_account(program_id, accounts[3].clone())?;
    let system = program::System::from_account(&accounts[4])?;

    holder.validate_owner(&operator)?;

    // We have to initialize the heap before creating Transaction object, but since
    // transaction's rlp itself is stored in the holder account, we have two options:
    // 1. Copy the rlp and initialize the heap right after the holder's header.
    //   This way, the space occupied by the rlp within holder will be reused.
    // 2. Don't copy the rlp, initialize the heap after transaction rlp in the holder.
    // The first option (chosen) saves the holder space in exchange for compute units.
    // The second option wastes the holder space (because transaction bytes will be
    // stored two times), but doesnt copy.
    let transaction_rlp_copy = {
        let holder_transaction_ref = holder.transaction();
        let mut transaction_copy = vec![0u8; holder_transaction_ref.len()];
        transaction_copy.copy_from_slice(&holder_transaction_ref);
        transaction_copy
    };
    holder.init_heap(0)?;

    let trx = boxx(Transaction::from_rlp(&transaction_rlp_copy)?);
    holder.validate_transaction(&trx)?;

    let origin = trx.recover_caller_address()?;

    log_data(&[b"HASH", &trx.hash()]);
    log_data(&[b"MINER", operator_balance.address().as_bytes()]);

    let accounts_db = AccountsDB::new(
        &accounts[5..],
        operator,
        Some(operator_balance),
        Some(system),
        Some(treasury),
    );

    let mut gasometer = Gasometer::new(U256::ZERO, accounts_db.operator())?;
    gasometer.record_solana_transaction_cost();
    gasometer.record_address_lookup_table(accounts);
    // TODO: this is probably invalid as we implicitly write to holder more (via heap allocations).
    gasometer.record_write_to_holder(&trx);

    super::transaction_execute::execute(accounts_db, gasometer, trx, origin)
}
