use crate::config::PAYMENT_TO_TREASURE;
use crate::debug::log_data;
use crate::error::Error;
use crate::gasometer::LAMPORTS_PER_SIGNATURE;
use crate::types::Transaction;
use crate::types::TransactionPayload;
use ethnum::U256;
use solana_program::{instruction::get_processed_sibling_instruction, pubkey, pubkey::Pubkey};
use std::convert::From;

// Because ComputeBudget program is not accessible through CPI, it's not a part of the standard
// solana_program library crate. Thus, we have to hardcode a couple of constants.
// The pubkey of the Compute Budget.
const COMPUTE_BUDGET_ADDRESS: Pubkey = pubkey!("ComputeBudget111111111111111111111111111111");
// The Compute Budget SetComputeUnitLimit instruction tag.
const COMPUTE_UNIT_LIMIT_TAG: u8 = 0x2;
// The Compute Budget SetComputeUnitPrice instruction tag.
const COMPUTE_UNIT_PRICE_TAG: u8 = 0x3;
// The default compute units limit for Solana transactions.
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 200_000;

// Conversion from "total micro lamports" to lamports.
const MICRO_LAMPORTS: u64 = 1_000_000;

/// Handles priority fee:
/// - Calculates and logs the priority fee in tokens.
pub fn handle_priority_fee(txn: &Transaction) -> Result<U256, Error> {
    let priority_fee_in_tokens = calc_priority_fee(txn)?;
    if priority_fee_in_tokens != U256::ZERO {
        log_data(&[b"PRIORITYFEE", &priority_fee_in_tokens.to_le_bytes()]);
    }

    return Ok(priority_fee_in_tokens);
}

/// Handles priority fee:
/// - Calculates the priority fee in tokens for iteration
/// - If there is some module from dividing of total priority fee on total gas used
/// --- than add this remain, because Ethereum API have two separate values:
/// ----- effective-gas-price,
/// ----- gas-used
/// --- and without rounding, Ethereum clients fail on gas usage calculations
pub fn finalize_priority_fee(
    txn: &Transaction,
    total_gas_used: U256,
    total_priority_fee_used: U256,
) -> Result<U256, Error> {
    let mut priority_fee_in_tokens = calc_priority_fee(txn)?;
    let total_priority_fee_used = total_priority_fee_used.saturating_add(priority_fee_in_tokens);
    if total_priority_fee_used == U256::ZERO {
        return Ok(U256::ZERO);
    }

    let max_priority_fee_per_gas = txn.max_priority_fee_per_gas().unwrap_or_default();
    let max_priority_fee = max_priority_fee_per_gas.saturating_mul(total_gas_used);

    let total_priority_fee_rest = max_priority_fee.saturating_sub(total_priority_fee_used);
    let rem_tokens = total_priority_fee_rest.wrapping_rem(total_gas_used);
    if rem_tokens != U256::ZERO {
        priority_fee_in_tokens = priority_fee_in_tokens.saturating_add(rem_tokens);
    }

    if priority_fee_in_tokens != U256::ZERO {
        log_data(&[b"PRIORITYFEE", &priority_fee_in_tokens.to_le_bytes()]);
    }

    return Ok(priority_fee_in_tokens);
}

/// Returns the amount of "priority fee in tokens" that User have to pay to the Operator.
/// - No-op for anything but DynamicFee or Scheduled transactions,
pub fn calc_priority_fee(txn: &Transaction) -> Result<U256, Error> {
    let (max_fee, max_priority_fee) = match txn.transaction {
        TransactionPayload::DynamicFee(ref payload) => {
            (payload.max_fee_per_gas, payload.max_priority_fee_per_gas)
        }
        TransactionPayload::Scheduled(ref payload) => {
            (payload.max_fee_per_gas, payload.max_priority_fee_per_gas)
        }
        _ => return Ok(U256::ZERO),
    };

    if max_priority_fee > max_fee {
        return Err(Error::PriorityFeeError(
            "max_priority_fee_per_gas > max_fee_per_gas".to_string(),
        ));
    }

    if max_fee == max_priority_fee {
        // If max_fee_per_gas == max_priority_fee_per_gas, we handle transaction as legacy:
        // - charge max_fee_per_gas * gas_used,
        // - do not charge any priority fee.
        return Ok(U256::ZERO);
    }

    if max_priority_fee == U256::ZERO {
        // If the User set priority fee to zero, the resulting priority fee is 0.
        return Ok(U256::ZERO);
    }

    let (cu_limit, cu_price) = get_compute_budget_priority_fee()?;

    let priority_gas_in_microlamports: u64 =
        cu_price
            .checked_mul(cu_limit as u64)
            .ok_or(Error::PriorityFeeError(
                "cu_limit * cu_price overflow".to_string(),
            ))?;
    let base_fee_per_gas = max_fee - max_priority_fee;
    let priority_fee_in_tokens = base_fee_per_gas
        .checked_mul(U256::from(priority_gas_in_microlamports))
        .and_then(|r| r.checked_div(U256::from(MICRO_LAMPORTS)))
        .ok_or(Error::PriorityFeeError(
            "base_fee_per_gas * priority_gas_in_microlamports overflow".to_string(),
        ))?;

    // Get minimum value of priority_fee_per_gas from what the User sets as max_priority_fee_per_gas
    // and what the operator paid as Compute Budget (as converted to gas tokens).
    const MAX_GAS: U256 = U256::new(LAMPORTS_PER_SIGNATURE as u128 + PAYMENT_TO_TREASURE as u128);
    Ok(priority_fee_in_tokens.min(max_priority_fee.saturating_mul(MAX_GAS)))
}

/// Extracts the data about compute units from instructions within the current transaction.
/// Returns the pair of (`compute_budget_unit_limit`, `compute_budget_unit_price`)
/// N.B. the `compute_budget_unit_price` is denominated in micro Lamports.
fn get_compute_budget_priority_fee() -> Result<(u32, u64), Error> {
    // Intent is to check first several instructions in hopes to find ComputeBudget ones.
    let max_idx = 5;

    let mut idx = 0;
    let mut compute_unit_limit: Option<u32> = None;
    let mut compute_unit_price: Option<u64> = None;
    while (compute_unit_limit.is_none() || compute_unit_price.is_none()) && idx < max_idx {
        let ixn_option = get_processed_sibling_instruction(idx);
        if ixn_option.is_none() {
            // If the current instruction is empty, break from the cycle.
            break;
        }

        let cur_ixn = ixn_option.unwrap();
        // Skip all instructions that do not target Compute Budget Program.
        if cur_ixn.program_id != COMPUTE_BUDGET_ADDRESS {
            idx += 1;
            continue;
        }

        // As of now, data of ComputeBudgetInstruction is always non-empty.
        // This is a sanity check to have a safe future-proof implementation.
        let tag = cur_ixn.data.first().unwrap_or(&0);
        match *tag {
            COMPUTE_UNIT_LIMIT_TAG => {
                compute_unit_limit = Some(u32::from_le_bytes(
                    cur_ixn.data[1..].try_into().map_err(|_| {
                        Error::PriorityFeeParsingError(
                            "Invalid format of compute unit limit.".to_string(),
                        )
                    })?,
                ));
            }
            COMPUTE_UNIT_PRICE_TAG => {
                compute_unit_price = Some(u64::from_le_bytes(
                    cur_ixn.data[1..].try_into().map_err(|_| {
                        Error::PriorityFeeParsingError(
                            "Invalid format of compute unit price.".to_string(),
                        )
                    })?,
                ));
            }
            _ => (),
        }
        idx += 1;
    }

    if compute_unit_price.is_none() {
        return Err(Error::PriorityFeeNotSpecified);
    }

    // Caller may not specify the compute unit limit, the default should take effect.
    if compute_unit_limit.is_none() {
        compute_unit_limit = Some(DEFAULT_COMPUTE_UNIT_LIMIT);
    }

    // Both are not none, it's safe to unwrap.
    Ok((compute_unit_limit.unwrap(), compute_unit_price.unwrap()))
}
