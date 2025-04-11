use crate::config::{
    BASE_ITERATIVE_TRANSACTION_COST, EXEC_ITERATION_COST, MINIMAL_ITERATION_COUNT,
};
use crate::error::Error;
use crate::types::Transaction;
use ethnum::U256;
use solana_program::{
    instruction::{
        get_processed_sibling_instruction, get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT,
    },
    pubkey,
    pubkey::Pubkey,
};

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
// The maximum value for compute units limit in Solana transactions.
const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;
// The default compute units price for Solana transactions
const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 0;
// The number of micro lamports in 1 lamport
const MICROS_PER_LAMPORT: u64 = 1_000_000;
// Compute Unit Price Unit encoded in gas-limit
const BASE_COMPUTE_UNIT_PRICE_UNIT: u64 = 10_500;
// The minimal value of Solana Priority Fee,
//   if the transaction requests the maximum number of Compute Units
const BASE_PRIORITY_GAS_UNIT: u64 =
    BASE_COMPUTE_UNIT_PRICE_UNIT * (MAX_COMPUTE_UNIT_LIMIT as u64) / MICROS_PER_LAMPORT;
// Length in the gas-limit header is packed by bits-batches
const HDR_BIT_LENGTH_BATCH: u64 = 2;
// Iterations length in the header of the gas-limit
const HDR_ITERATION_COUNT_LENGTH: u64 = 3;
const HDR_ITERATION_COUNT_MASK: u64 = bitmask(HDR_ITERATION_COUNT_LENGTH);
// Compute Unit price length in the header of the gas-limit
const HDR_GAS_UNIT_COUNT_LENGTH: u64 = 4;
const HDR_GAS_UNIT_COUNT_MASK: u64 = bitmask(HDR_GAS_UNIT_COUNT_LENGTH);
// The total length of the header of the gas-limit
const HDR_TOTAL_LENGTH: u64 = HDR_ITERATION_COUNT_LENGTH + HDR_GAS_UNIT_COUNT_LENGTH;
const HDR_TOTAL_MASK: u64 = bitmask(HDR_TOTAL_LENGTH);

const fn bitmask(length: u64) -> u64 {
    (1u64 << length) - 1
}

/// Returns the Gas used for Solana Priority Fee.
pub fn calc_priority_gas(trx: &Transaction) -> Result<u64, Error> {
    let (cu_limit, cu_price) = get_compute_budget_priority_fee()?;
    if cu_price == 0 || cu_limit == 0 {
        return Ok(0);
    }

    let priority_gas: u64 = cu_price
        .checked_mul(u64::from(cu_limit))
        .and_then(|r| r.checked_div(MICROS_PER_LAMPORT))
        .ok_or(Error::PriorityFeeError(
            "cu_limit * cu_price / 10^6 overflow".to_string(),
        ))?;

    let gas_limit = trx.gas_limit();
    if gas_limit > U256::from(u64::MAX) {
        return Err(Error::GasLimitOverflow(gas_limit));
    }

    let trx_max_priority_gas = get_max_priority_gas(gas_limit.as_u64());

    Ok(priority_gas.min(trx_max_priority_gas))
}

/// Extracts the maximum Solana Priority Fee encoded in gas-limit of Neon transaction  
fn get_max_priority_gas(gas_limit: u64) -> u64 {
    // unpack header
    // {
    //   int gas_unit_cnt:4;
    //   int iter_cnt:3;
    // }
    let pkt_hdr = (gas_limit & HDR_TOTAL_MASK) ^ HDR_TOTAL_MASK;
    let iter_cnt_len = unpack_hdr_value(pkt_hdr, HDR_ITERATION_COUNT_MASK);
    let gas_unit_cnt_len = unpack_hdr_value(
        pkt_hdr >> HDR_ITERATION_COUNT_LENGTH,
        HDR_GAS_UNIT_COUNT_MASK,
    );

    // After header there are real values
    let pkt_cu_cost_len = iter_cnt_len + gas_unit_cnt_len;
    let pkt_cu_cost_mask = bitmask(pkt_cu_cost_len);
    let pkt_cu_cost = ((gas_limit >> HDR_TOTAL_LENGTH) & pkt_cu_cost_mask) ^ pkt_cu_cost_mask;

    let iter_cnt = unpack_length(pkt_cu_cost, iter_cnt_len);
    let total_iter_cnt = iter_cnt + MINIMAL_ITERATION_COUNT;
    let gas_unit_cnt = unpack_length(pkt_cu_cost >> iter_cnt_len, gas_unit_cnt_len);

    // Calculate the maximum gas for one iteration
    let Some(iter_priority_gas) = BASE_PRIORITY_GAS_UNIT.checked_mul(gas_unit_cnt + 1) else {
        return 0;
    };

    // Next steps validate overflows
    let Some(trx_priority_gas) = total_iter_cnt.checked_mul(iter_priority_gas) else {
        return 0;
    };
    let Some(trx_exec_gas) = iter_cnt.checked_mul(EXEC_ITERATION_COST) else {
        return 0;
    };

    if let Some(base_trx_gas) = gas_limit
        .saturating_sub(trx_priority_gas)
        .checked_sub(trx_exec_gas)
    {
        if BASE_ITERATIVE_TRANSACTION_COST <= base_trx_gas {
            return iter_priority_gas;
        }
    }
    0
}

fn unpack_hdr_value(value: u64, mask: u64) -> u64 {
    ((value & mask) + 1) * HDR_BIT_LENGTH_BATCH
}

fn unpack_length(value: u64, mask_len: u64) -> u64 {
    value & bitmask(mask_len)
}

/// Extracts the data about compute units from instructions within the current transaction.
/// Returns the pair of (`compute_budget_unit_limit`, `compute_budget_unit_price`)
/// N.B. the `compute_budget_unit_price` is denominated in micro Lamports.
fn get_compute_budget_priority_fee() -> Result<(u32, u64), Error> {
    // Intent is to check first several instructions in hopes to find ComputeBudget ones.
    let max_idx = 5;

    // The reason to forbid the calls for DynamicFee transactions - priority fee calculation
    // uses get_processed_sibling_instruction syscall which doesn't work well for CPI.
    let is_root_transaction = get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT;
    if !is_root_transaction {
        return Ok((0, 0));
    }

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
        compute_unit_price = Some(DEFAULT_COMPUTE_UNIT_PRICE);
    }

    // Caller may not specify the compute unit limit, the default should take effect.
    if compute_unit_limit.is_none() {
        compute_unit_limit = Some(DEFAULT_COMPUTE_UNIT_LIMIT);
    }

    // Both are not none, it's safe to unwrap.
    Ok((compute_unit_limit.unwrap(), compute_unit_price.unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bit_length(value: u64) -> u64 {
        64u64 - u64::from(value.leading_zeros())
    }

    fn calc_gas_unit_cnt(cu_price: u64) -> u64 {
        0.max(cu_price - 1) / BASE_COMPUTE_UNIT_PRICE_UNIT
    }

    fn calc_iter_cnt(iter_cnt: u64) -> u64 {
        iter_cnt - MINIMAL_ITERATION_COUNT
    }

    fn calc_pkt_len(value: u64) -> u64 {
        (bit_length(value) / HDR_BIT_LENGTH_BATCH + 1) * HDR_BIT_LENGTH_BATCH
    }

    fn calc_hdr_len(value: u64) -> u64 {
        value / HDR_BIT_LENGTH_BATCH - 1
    }

    fn calc_pkt_cu_cost_len(gas_unit_cnt: u64, iter_cnt: u64) -> u64 {
        HDR_TOTAL_LENGTH + calc_pkt_len(iter_cnt) + calc_pkt_len(gas_unit_cnt)
    }

    fn calc_pkt_cu_cost(gas_unit_cnt: u64, iter_cnt: u64, pkt_cu_cost_len: u64) -> u64 {
        let gas_unit_len = calc_pkt_len(gas_unit_cnt);
        let iter_cnt_len = calc_pkt_len(iter_cnt);

        let mut pkt_cu_cost = gas_unit_cnt;
        pkt_cu_cost <<= iter_cnt_len;
        pkt_cu_cost |= iter_cnt;

        pkt_cu_cost <<= HDR_GAS_UNIT_COUNT_LENGTH;
        pkt_cu_cost |= calc_hdr_len(gas_unit_len);
        pkt_cu_cost <<= HDR_ITERATION_COUNT_LENGTH;
        pkt_cu_cost |= calc_hdr_len(iter_cnt_len);

        pkt_cu_cost ^ bitmask(pkt_cu_cost_len)
    }

    fn calc_trx_cu_cost(gas_unit_cnt: u64, iter_cnt: u64) -> u64 {
        let iter_cu_cost = BASE_PRIORITY_GAS_UNIT * (gas_unit_cnt + 1);
        (iter_cnt + MINIMAL_ITERATION_COUNT) * iter_cu_cost
    }

    fn calc_trx_cost(base_gas: u64, cu_price: u64, base_iter_cnt: u64) -> u64 {
        let iter_cnt = calc_iter_cnt(base_iter_cnt);
        let gas_unit_cnt = calc_gas_unit_cnt(cu_price);
        let pkt_cu_cost_len = calc_pkt_cu_cost_len(gas_unit_cnt, iter_cnt);
        let pkt_cu_cost = calc_pkt_cu_cost(gas_unit_cnt, iter_cnt, pkt_cu_cost_len);

        let min_trx_cost = base_gas + calc_trx_cu_cost(gas_unit_cnt, iter_cnt);

        let mut high_trx_cost = min_trx_cost >> pkt_cu_cost_len;
        let trx_cost = high_trx_cost << pkt_cu_cost_len | pkt_cu_cost;
        if trx_cost > min_trx_cost {
            return trx_cost;
        }

        let bit_len = bit_length(high_trx_cost);
        let mut has_bit = false;
        for i in 0..bit_len {
            let bit = 1u64 << i;

            if high_trx_cost & bit == 0 {
                high_trx_cost |= bit;
                has_bit = true;
                break;
            }
        }

        if !has_bit {
            high_trx_cost = 1 << bit_len;
        }

        high_trx_cost << pkt_cu_cost_len | pkt_cu_cost
    }

    #[test]
    fn test_bitmask() {
        assert_eq!(bitmask(0), 0b0);
        assert_eq!(bitmask(1), 0b1);
        assert_eq!(bitmask(4), 0b1111);
        assert_eq!(bitmask(8), 0b1111_1111);
    }

    #[test]
    fn test_bit_length() {
        assert_eq!(bit_length(0b1), 1);
        assert_eq!(bit_length(0b111), 3);
        assert_eq!(bit_length(0b1111), 4);
        assert_eq!(bit_length(0b1111_1111), 8);
    }

    #[test]
    fn test_unpack_hdr_value() {
        assert_eq!(
            unpack_hdr_value(0b000_1000, 0b1111),
            0b1001 * HDR_BIT_LENGTH_BATCH
        );
        assert_eq!(
            unpack_hdr_value(0b000_0100, 0b111),
            0b101 * HDR_BIT_LENGTH_BATCH
        );
    }

    #[test]
    fn test_unpack_length() {
        assert_eq!(unpack_length(0b111_1111, 4), 0b1111);
        assert_eq!(unpack_length(0b111_1111, 3), 0b111);
    }

    #[test]
    fn test_min_trx_cost() {
        let min_trx_cost = calc_trx_cost(
            BASE_ITERATIVE_TRANSACTION_COST,
            BASE_COMPUTE_UNIT_PRICE_UNIT,
            MINIMAL_ITERATION_COUNT,
        );
        assert_eq!(min_trx_cost, 0x127ff);

        let trx_max_priority_gas = get_max_priority_gas(min_trx_cost);
        assert_eq!(trx_max_priority_gas, BASE_PRIORITY_GAS_UNIT);
    }

    #[test]
    fn test_unpack_max_trx_gas_gas() {
        for base_gas in (BASE_ITERATIVE_TRANSACTION_COST
            ..BASE_ITERATIVE_TRANSACTION_COST + 1_000_000)
            .step_by(100_000)
        {
            for base_iter_cnt in 1..200 {
                for cu_price in (BASE_COMPUTE_UNIT_PRICE_UNIT..BASE_COMPUTE_UNIT_PRICE_UNIT * 100)
                    .step_by(10_000)
                {
                    let gas = base_gas + base_iter_cnt * EXEC_ITERATION_COST;
                    let iter_cnt = base_iter_cnt + MINIMAL_ITERATION_COUNT;

                    let trx_cost = calc_trx_cost(gas, cu_price, iter_cnt);
                    assert!(trx_cost > 0);

                    let gas_unit_cnt = calc_gas_unit_cnt(cu_price);
                    if cu_price > BASE_COMPUTE_UNIT_PRICE_UNIT {
                        assert!(gas_unit_cnt > 0);
                    }

                    let trx_max_priority_gas = get_max_priority_gas(trx_cost);
                    assert!(trx_max_priority_gas > 0);

                    assert_eq!(
                        trx_max_priority_gas,
                        BASE_PRIORITY_GAS_UNIT * (gas_unit_cnt + 1)
                    );
                }
            }
        }
    }
}
