use solana_program::{account_info::AccountInfo, pubkey::Pubkey, sysvar::instructions};
use std::str::FromStr;

use crate::error::Error;

// Because ComputeBudget program is not accessible through CPI, it's not a part of the standard
// solana_program library crate. Thus, we have to hardcode a couple of constants.
// The pubkey of the Compute Budget.
const COMPUTE_BUDGET_ADDRESS: &str = "ComputeBudget111111111111111111111111111111";
// The Compute Budget SetComputeUnitLimit instruction tag.
const COMPUTE_UNIT_LIMIT_TAG: u8 = 0x2;
// The Compute Budget SetComputeUnitPrice instruction tag.
const COMPUTE_UNIT_PRICE_TAG: u8 = 0x3;
// The default compute units limit for Solana transactions.
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 200000;

/// Extracts the instructions related to compute budget (via Sysvar account).
/// Returns the pair of (compute_budget_unit_limit, compute_budget_unit_price)
pub fn get_compute_budget_priority_fee(sysvar_account: &AccountInfo) -> Result<(u32, u64), Error> {
    let compute_budget_account_pubkey = Pubkey::from_str(COMPUTE_BUDGET_ADDRESS).map_err(|_| {
        Error::PriorityFeeParsingError("Invalid Compute budget address.".to_owned())
    })?;
    // Intent is to check all the instructions before the current one.
    let max_idx = instructions::load_current_index_checked(sysvar_account)? as usize;

    let mut idx = 0;
    let mut compute_unit_limit: Option<u32> = None;
    let mut compute_unit_price: Option<u64> = None;
    while (compute_unit_limit.is_none() || compute_unit_price.is_none()) && idx < max_idx {
        let cur_ixn = instructions::load_instruction_at_checked(idx, sysvar_account)?;

        // Skip all instructions that do not target Compute Budget Program.
        if cur_ixn.program_id != compute_budget_account_pubkey {
            idx += 1;
            continue;
        }

        // As of now, data of ComputeBudgetInstruction is always non-empty.
        // This is a sanity check to have a safe future-proof implementation.
        let tag = cur_ixn.data.get(0).unwrap_or(&0);
        match *tag {
            COMPUTE_UNIT_LIMIT_TAG => {
                compute_unit_limit = Some(u32::from_le_bytes(
                    cur_ixn.data[1..].try_into().map_err(|_| {
                        Error::PriorityFeeParsingError(
                            "Invalid format of compute unit limit.".to_owned(),
                        )
                    })?,
                ));
            }
            COMPUTE_UNIT_PRICE_TAG => {
                compute_unit_price = Some(u64::from_le_bytes(
                    cur_ixn.data[1..].try_into().map_err(|_| {
                        Error::PriorityFeeParsingError(
                            "Invalid format of compute unit price.".to_owned(),
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

    Ok((compute_unit_limit.unwrap(), compute_unit_price.unwrap()))
}
