use crate::account::program::System;
use crate::account::{token, NodeInitializer, Signer, TransactionTree, Treasury, TreeInitializer};
use crate::config::SOL_CHAIN_ID;
use crate::error::{Error, Result};
use crate::types::Address;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::clock::Clock;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;

use super::scheduled_transaction_create::{
    payment_from_balance, payment_from_signer, validate_balance, validate_pool,
};

fn parse_instruction(signer: &Signer, instruction: &[u8]) -> TreeInitializer {
    const HEADER_LEN: usize = 72;
    const CHUNK_LEN: usize = 100;

    let header = arrayref::array_ref![instruction, 0, HEADER_LEN];
    let message = &instruction[HEADER_LEN..];

    assert!(!message.is_empty());
    assert!(message.len() % CHUNK_LEN == 0);

    let (nonce, max_fee_per_gas, max_priority_fee_per_gas) =
        arrayref::array_refs![header, 8, 32, 32];

    let mut nodes = vec![];
    for chunk in message.chunks_exact(CHUNK_LEN) {
        let chunk = arrayref::array_ref![chunk, 0, CHUNK_LEN];
        let (gas_limit, value, child_index, success_limit, hash) =
            arrayref::array_refs![chunk, 32, 32, 2, 2, 32];

        nodes.push(NodeInitializer {
            transaction_hash: *hash,
            child: u16::from_le_bytes(*child_index),
            success_execute_limit: u16::from_le_bytes(*success_limit),
            gas_limit: U256::from_be_bytes(*gas_limit),
            value: U256::from_be_bytes(*value),
        })
    }

    TreeInitializer {
        payer: Address::from_solana_address(signer.key),
        nonce: u64::from_be_bytes(*nonce),
        chain_id: SOL_CHAIN_ID,
        max_fee_per_gas: U256::from_be_bytes(*max_fee_per_gas),
        max_priority_fee_per_gas: U256::from_be_bytes(*max_priority_fee_per_gas),
        nodes,
    }
}

fn calculate_required_balance(init_data: &TreeInitializer) -> Result<U256> {
    let mut total_balance = U256::ZERO;

    for node in &init_data.nodes {
        let Some(gas) = node.gas_limit.checked_mul(init_data.max_fee_per_gas) else {
            return Err(Error::TreeAccountTxInvalidData);
        };
        let Some(node_balance) = gas.checked_add(node.value) else {
            return Err(Error::TreeAccountTxInvalidData);
        };
        let Some(new_total_balance) = total_balance.checked_add(node_balance) else {
            return Err(Error::TreeAccountTxInvalidData);
        };

        total_balance = new_total_balance;
    }

    Ok(total_balance)
}

/// Execute Ethereum transaction in a single Solana transaction
pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Schedule Multiple Transactions");

    // Instruction data
    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let message = &instruction[4..];

    // Accounts
    let signer = Signer::from_account(&accounts[0])?;
    let balance = accounts[1].clone();
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let tree = accounts[3].clone();
    let pool = token::State::from_account(&accounts[4])?;
    let system = System::from_account(&accounts[5])?;

    let init_data = parse_instruction(&signer, message);
    let required_balance = calculate_required_balance(&init_data)?;

    validate_balance(&balance, init_data.payer)?;
    validate_pool(&pool)?;

    // Create Tree Account
    let rent = Rent::get()?;
    let clock = Clock::get()?;

    let mut tree = TransactionTree::create(init_data, tree, &treasury, &system, &rent, &clock)?;

    let required_balance = payment_from_balance(&mut tree, balance, required_balance)?;
    payment_from_signer(&mut tree, &signer, &pool, &system, required_balance)?;

    Ok(())
}
