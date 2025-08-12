use std::convert::TryInto;

use crate::account::{Operator, Root};
use crate::config::{HOLDER_MSG_SIZE, LAMPORTS_PER_SIGNATURE, LAST_ITERATION_COST};
use crate::error::Error;
use crate::priority_gas_calculator::calc_priority_gas;
use crate::types::EncodedTransaction;
use ethnum::U256;
use solana_program::account_info::AccountInfo;

const WRITE_TO_HOLDER_TRX_COST: u64 = LAMPORTS_PER_SIGNATURE;

pub struct Gasometer {
    gas: u64,
    refund: u64,
    operator_balance: u64,
}

impl Gasometer {
    #[must_use]
    pub fn new(operator: &Operator) -> Self {
        Self {
            gas: 0_u64,
            refund: 0_u64,
            operator_balance: operator.lamports(),
        }
    }

    #[must_use]
    pub fn collect_gas(&mut self, operator: &Operator) -> U256 {
        self.record_operator_expenses(operator);
        let gas = U256::from(self.gas.saturating_sub(self.refund));

        *self = Self::new(operator); // Reset the gasometer
        gas
    }

    pub fn refund_lamports(&mut self, lamports: u64) {
        self.refund = self.refund.saturating_add(lamports);
    }

    fn record_operator_expenses(&mut self, operator: &Operator) {
        let expenses = self.operator_balance.saturating_sub(operator.lamports());

        self.gas = self.gas.saturating_add(expenses);
    }

    pub fn record_solana_transaction_cost(&mut self, gas_limit: U256) -> Result<(), Error> {
        self.gas = self.gas.saturating_add(LAMPORTS_PER_SIGNATURE);

        let priority_gas = calc_priority_gas(gas_limit)?;
        self.gas = self.gas.saturating_add(priority_gas);

        Ok(())
    }

    pub fn record_write_to_holder(&mut self, transaction: &EncodedTransaction) {
        let size: u64 = transaction.rlp_len().try_into().expect("usize is 8 bytes");
        let cost: u64 = size
            .div_ceil(HOLDER_MSG_SIZE)
            .saturating_mul(WRITE_TO_HOLDER_TRX_COST);

        self.gas = self.gas.saturating_add(cost);
    }

    pub fn record_address_lookup_table(&mut self, accounts: &[AccountInfo]) {
        const MIN_ACCOUNTS_TO_USE_ALT: usize = 30;
        const ACCOUNTS_PER_ALT_EXTEND: usize = 30;

        if accounts.len() < MIN_ACCOUNTS_TO_USE_ALT {
            return;
        }

        let extend_count = accounts.len().div_ceil(ACCOUNTS_PER_ALT_EXTEND);
        // create_alt + extend_alt + deactivate_alt + close_alt
        let cost = (extend_count + 3) as u64 * LAMPORTS_PER_SIGNATURE;

        self.gas = self.gas.saturating_add(cost);
    }

    pub fn record_cancel_gas(&mut self, root: &Root) -> Result<(), Error> {
        // TODO: Figure out wtf is going on here

        let priority_gas = calc_priority_gas(root.gas_limit())?;
        let cancel_cost = LAST_ITERATION_COST.saturating_add(priority_gas);

        let gas_available = root.gas_available().try_into()?;
        let cancel_cost = cancel_cost.min(gas_available);

        self.gas = self.gas.saturating_add(cancel_cost);

        Ok(())
    }
}
