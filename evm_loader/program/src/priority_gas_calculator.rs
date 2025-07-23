use crate::config::{
    BASE_ITERATIVE_TRANSACTION_COST, EXEC_ITERATION_COST, MINIMAL_ITERATION_COUNT,
};
use crate::error::Error;
use ethnum::U256;
use solana_compute_budget_interface::check_id as check_compute_budget_id;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_program::borsh1::try_from_slice_unchecked;
use solana_program::instruction::{
    get_processed_sibling_instruction, get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT,
};

// The default compute units limit for Solana transactions.
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 200_000;
// The maximum value for compute units limits in Solana transactions.
const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;
// The number of micro lamports in 1 lamport
const MICRO_LAMPORTS_PER_LAMPORT: u64 = 1_000_000;
// Compute Unit Price Unit encoded in gas-limit
const BASE_COMPUTE_UNIT_PRICE_UNIT: u64 = 10_500;
// The minimal value of Solana Priority Fee, based on BASE_COMPUTE_UNIT_PRICE_UNIT
const BASE_PRIORITY_GAS_UNIT: u64 =
    BASE_COMPUTE_UNIT_PRICE_UNIT * (MAX_COMPUTE_UNIT_LIMIT as u64) / MICRO_LAMPORTS_PER_LAMPORT;
// Length in the gas-limit header is packed by bits-batches
const HDR_BIT_LENGTH_BATCH: u64 = 2;
// Iteration counter length in the header of the gas-limit
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
pub fn calc_priority_gas(gas_limit: U256) -> Result<u64, Error> {
    let priority_gas = get_compute_budget_priority_fee()?;
    if priority_gas == 0 {
        return Ok(0);
    }

    if gas_limit >= U256::from(u64::MAX) {
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

    // After the header there are real values
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

fn calc_solana_priority_fee(
    compute_unit_price: u64,
    compute_unit_limit: u32,
) -> Result<u64, Error> {
    // Copy-paste from
    //   https://docs.rs/solana-compute-budget/2.2.11/src/solana_compute_budget/compute_budget_limits.rs.html#46-54
    //
    // Reasons for copy-paste:
    // 1. Source code migrates from one module to another in different versions:
    //    https://docs.rs/solana-compute-budget/2.1.21/src/solana_compute_budget/prioritization_fee.rs.html#20-26)
    // 2. It's a private function in the last version.
    // 3. The original code depends on the `solana-sdk` crate, which can't be used in Solana programs
    let micro_lamport_fee: u128 =
        u128::from(compute_unit_price).saturating_mul(u128::from(compute_unit_limit));

    micro_lamport_fee
        .saturating_add(u128::from(MICRO_LAMPORTS_PER_LAMPORT - 1))
        .checked_div(u128::from(MICRO_LAMPORTS_PER_LAMPORT))
        .and_then(|fee| u64::try_from(fee).ok())
        .ok_or(Error::PriorityFeeError(
            "ComputeUnitLimit * ComputeUnitPrice / MicroLamportsPerLamport overflow",
        ))
}

/// Extracts the data about compute units from instructions within the current transaction.
/// Returns the Solana Priority Fee
fn get_compute_budget_priority_fee() -> Result<u64, Error> {
    // get_processed_sibling_instruction syscall doesn't work well for CPI.
    let is_root_transaction = get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT;
    if !is_root_transaction {
        return Ok(0);
    }

    let mut compute_unit_limit: Option<u32> = None;
    let mut compute_unit_price: Option<u64> = None;

    // The intent is to check the first several instructions in hopes to find ComputeBudget ones.
    for idx in 0..5 {
        let Some(cur_ixn) = get_processed_sibling_instruction(idx) else {
            break;
        };

        // Skip all instructions that do not target Compute Budget Program.
        if !check_compute_budget_id(&cur_ixn.program_id) {
            continue;
        }

        // As of now, data of ComputeBudgetInstruction is always non-empty.
        // This is a sanity check to have a safe future-proof implementation.
        match try_from_slice_unchecked(&cur_ixn.data) {
            Ok(ComputeBudgetInstruction::SetComputeUnitLimit(value)) => {
                compute_unit_limit = Some(value);
                if compute_unit_price.is_some() {
                    break;
                }
            }
            Ok(ComputeBudgetInstruction::SetComputeUnitPrice(value)) => {
                compute_unit_price = Some(value);
                if compute_unit_limit.is_some() {
                    break;
                }
            }
            _ => (),
        }
    }

    // No Priority Fee case
    if compute_unit_price.is_none() {
        return Ok(0);
    }

    // Caller may not specify the compute unit limit, the default should take effect.
    if compute_unit_limit.is_none() {
        compute_unit_limit = Some(DEFAULT_COMPUTE_UNIT_LIMIT);
    }

    calc_solana_priority_fee(compute_unit_price.unwrap(), compute_unit_limit.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bit_length(value: u64) -> u64 {
        64u64 - u64::from(value.leading_zeros())
    }

    fn calc_gas_unit_cnt(cu_price: u64) -> u64 {
        cu_price.saturating_sub(1) / BASE_COMPUTE_UNIT_PRICE_UNIT
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
    fn test_base_gas_unit() {
        let priority_gas_unit: u64 =
            calc_solana_priority_fee(BASE_COMPUTE_UNIT_PRICE_UNIT, MAX_COMPUTE_UNIT_LIMIT)
                .unwrap_or(u64::MAX);

        assert_eq!(BASE_PRIORITY_GAS_UNIT, priority_gas_unit);
        assert_eq!(BASE_PRIORITY_GAS_UNIT, 14_700);
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
