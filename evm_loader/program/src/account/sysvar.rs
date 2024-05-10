use crate::error::Error;
use crate::gasometer::LAMPORTS_PER_SIGNATURE;
use ethnum::U256;
use solana_program::program_error::ProgramError;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, sysvar::instructions};
use std::convert::From;
use std::ops::Deref;
use std::str::FromStr;

// Because ComputeBudget program is not accessible through CPI, it's not a part of the standard
// solana_program library crate. Thus, we have to hardcode a couple of constants.
// The pubkey of the Compute Budget.
const COMPUTE_BUDGET_ADDRESS: &str = "ComputeBudget111111111111111111111111111111";
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

pub struct Sysvar<'a>(&'a AccountInfo<'a>);

impl<'a> From<&Sysvar<'a>> for &'a AccountInfo<'a> {
    fn from(f: &Sysvar<'a>) -> Self {
        f.0
    }
}

impl<'a> Sysvar<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self, ProgramError> {
        if !instructions::check_id(info.key) {
            return Err!(ProgramError::InvalidArgument; "Account {} - is not sysvar program", info.key);
        }

        Ok(Self(info))
    }

    /// Extracts the data about compute units from instructions within the current transaction.
    /// Returns the pair of (`compute_budget_unit_limit`, `compute_budget_unit_price`)
    /// N.B. the `compute_budget_unit_price` is denominated in micro Lamports.
    pub fn get_compute_budget_priority_fee(&self) -> Result<(u32, u64), Error> {
        let compute_budget_account_pubkey =
            Pubkey::from_str(COMPUTE_BUDGET_ADDRESS).map_err(|_| {
                Error::PriorityFeeParsingError("Invalid Compute budget address.".to_string())
            })?;
        // Intent is to check all the instructions before the current one.
        let max_idx = instructions::load_current_index_checked(self.0)? as usize;

        let mut idx = 0;
        let mut compute_unit_limit: Option<u32> = None;
        let mut compute_unit_price: Option<u64> = None;
        while (compute_unit_limit.is_none() || compute_unit_price.is_none()) && idx < max_idx {
            let cur_ixn = instructions::load_instruction_at_checked(idx, self.0)?;

            // Skip all instructions that do not target Compute Budget Program.
            if cur_ixn.program_id != compute_budget_account_pubkey {
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
        &self,
        trx_max_priority_fee_per_gas: U256,
        trx_max_fee_per_gas: U256,
    ) -> Result<(), Error> {
        let (cu_limit, cu_price) = self.get_compute_budget_priority_fee()?;
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
            .ok_or(Error::PriorityFeeError("base_fee_per_gas * ".into()))?;

        if actual_priority_fee_per_gas >= trx_max_priority_fee_per_gas {
            Ok(())
        } else {
            Err(Error::PriorityFeeError(
                "actual_priority_fee_per_gas < max_priority_fee_per_gas".to_string(),
            ))
        }
    }
}

impl<'a> Deref for Sysvar<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
