use crate::error::Error;
use crate::gasometer::LAMPORTS_PER_SIGNATURE;
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

// The divisor in the conversion from priority fee in microlamports to priority fee per gas.
// This divisor includes the tolerance_level := 0.9 which is an allowed discrepancy between
// the actual priority fee per gas as paid by the Operator and the priority fee per gas
// as set by the User in the transaction.
const CONVERSION_DIVISOR: u64 = LAMPORTS_PER_SIGNATURE * 900_000;

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

/// Checks that priority fee as set by the Operator is accurate to what User set as `max_priority_fee_per_gas`.
pub fn validate_priority_fee(
    trx_max_priority_fee_per_gas: U256,
    trx_max_fee_per_gas: U256,
) -> Result<(), Error> {
    if trx_max_priority_fee_per_gas == U256::ZERO {
        // If the User set priority fee to zero, there's nothing to validate.
        return Ok(());
    }

    let (cu_limit, cu_price) = get_compute_budget_priority_fee()?;
    let priority_fee: U256 = cu_price
        .checked_mul(u64::from(cu_limit))
        .ok_or(Error::PriorityFeeError(
            "cu_limit * cu_price overflow".to_string(),
        ))?
        .into();
    let base_fee_per_gas = trx_max_fee_per_gas - trx_max_priority_fee_per_gas;

    let actual_priority_fee_per_gas = base_fee_per_gas
        .checked_mul(priority_fee)
        .and_then(|res| res.checked_div(CONVERSION_DIVISOR.into()))
        .ok_or(Error::PriorityFeeError(
            "actual priority_fee_per_gas overflow".into(),
        ))?;

    if actual_priority_fee_per_gas >= trx_max_priority_fee_per_gas {
        Ok(())
    } else {
        Err(Error::PriorityFeeError(
            "actual_priority_fee_per_gas < max_priority_fee_per_gas".to_string(),
        ))
    }
}
