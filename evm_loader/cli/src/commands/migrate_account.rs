use log::{error, info, debug};

use solana_sdk::{
    commitment_config::{CommitmentConfig},
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    transaction::Transaction,
    compute_budget::ComputeBudgetInstruction,
};

use solana_cli::{
    checks::{check_account_for_fee},
};

use spl_associated_token_account::get_associated_token_address;

use evm::{H160};

use evm_loader::config::{
    token_mint,
    COMPUTE_BUDGET_UNITS,
    COMPUTE_BUDGET_HEAP_FRAME,
    REQUEST_UNITS_ADDITIONAL_FEE,
};

use crate::{
    Config,
    NeonCliError,
    NeonCliResult,
    make_solana_program_address,
};

/// Executes subcommand `migrate-account`.
#[allow(clippy::unnecessary_wraps)]
pub fn execute(
    config: &Config,
    ether_address: &H160,
) -> NeonCliResult {
    let (ether_pubkey, nonce) = make_solana_program_address(ether_address, &config.evm_loader);

    // Check existence of ether account
    config.rpc_client.get_account(&ether_pubkey)
        .map_err(|e| {
            error!("{}", e);
            NeonCliError::AccountNotFoundAtAddress(*ether_address)
        })?;

    let instructions = vec![
        ComputeBudgetInstruction::request_units(COMPUTE_BUDGET_UNITS, REQUEST_UNITS_ADDITIONAL_FEE),
        ComputeBudgetInstruction::request_heap_frame(COMPUTE_BUDGET_HEAP_FRAME),
        migrate_account_instruction(
            config,
            ether_pubkey,
    )];

    let mut finalize_message = Message::new(&instructions, Some(&config.signer.pubkey()));
    let blockhash = config.rpc_client.get_latest_blockhash()?;
    finalize_message.recent_blockhash = blockhash;

    check_account_for_fee(
        &config.rpc_client,
        &config.signer.pubkey(),
        &finalize_message
    )?;

    let mut finalize_tx = Transaction::new_unsigned(finalize_message);

    finalize_tx.try_sign(&[&*config.signer], blockhash)?;
    debug!("signed: {:x?}", finalize_tx);

    config.rpc_client.send_and_confirm_transaction_with_spinner_and_commitment(
        &finalize_tx,
        CommitmentConfig::confirmed(),
    )?;

    info!("{}", serde_json::json!({
        "ether address": hex::encode(ether_address),
        "nonce": nonce,
    }));

    Ok(())
}

/// Returns instruction to migrate Ethereum account.
fn migrate_account_instruction(
    config: &Config,
    ether_pubkey: Pubkey,
) -> Instruction {
    let token_authority = Pubkey::find_program_address(&[b"Deposit"], &config.evm_loader).0;
    let token_pool_pubkey = get_associated_token_address(&token_authority, &token_mint::id());
    let ether_token_pubkey = get_associated_token_address(&ether_pubkey, &token_mint::id());

    Instruction::new_with_bincode(
        config.evm_loader,
        &(26_u8),
        vec![
            AccountMeta::new(config.signer.pubkey(), true),
            AccountMeta::new(ether_pubkey, false),
            AccountMeta::new(ether_token_pubkey, false),
            AccountMeta::new(token_pool_pubkey, false),
            AccountMeta::new_readonly(token_authority, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}
