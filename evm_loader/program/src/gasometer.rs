use std::convert::TryInto;

use crate::account::Operator;
use crate::config::{HOLDER_MSG_SIZE, LAMPORTS_PER_SIGNATURE, TREE_ACCOUNT_FINISH_TRANSACTION_GAS};
use crate::error::Error;
use crate::priority_gas_calculator::calc_priority_gas;
use crate::types::Transaction;
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::program_error::ProgramError;

const WRITE_TO_HOLDER_TRX_COST: u64 = LAMPORTS_PER_SIGNATURE;

pub struct Gasometer {
    paid_gas: U256,
    gas: u64,
    refund: u64,
    operator_balance: u64,
}

impl Gasometer {
    pub fn new(paid_gas: U256, operator: &Operator) -> Result<Self, ProgramError> {
        Ok(Self {
            paid_gas,
            gas: 0_u64,
            refund: 0_u64,
            operator_balance: operator.lamports(),
        })
    }

    #[must_use]
    pub fn used_gas(&self) -> U256 {
        U256::from(self.gas.saturating_sub(self.refund))
    }

    #[must_use]
    pub fn used_gas_total(&self) -> U256 {
        self.paid_gas.saturating_add(self.used_gas())
    }

    pub fn refund_lamports(&mut self, lamports: u64) {
        self.refund = self.refund.saturating_add(lamports);
    }

    pub fn record_operator_expenses(&mut self, operator: &Operator) {
        let expenses = self.operator_balance.saturating_sub(operator.lamports());

        self.gas = self.gas.saturating_add(expenses);
    }

    pub fn record_solana_transaction_cost(&mut self, trx: &Transaction) -> Result<(), Error> {
        self.gas = self.gas.saturating_add(LAMPORTS_PER_SIGNATURE);

        let priority_gas = calc_priority_gas(trx)?;
        self.gas = self.gas.saturating_add(priority_gas);

        Ok(())
    }

    pub fn record_write_to_holder(&mut self, trx: &Transaction) {
        let size: u64 = trx.rlp_len().try_into().expect("usize is 8 bytes");
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

    pub fn record_scheduled_transaction_finish(&mut self) {
        // real gas usage happens in finish instruction
        //  here we just reserve the gas for finish instruction
        self.gas = self.gas.saturating_add(TREE_ACCOUNT_FINISH_TRANSACTION_GAS);
        self.refund_lamports(TREE_ACCOUNT_FINISH_TRANSACTION_GAS);
    }
}
