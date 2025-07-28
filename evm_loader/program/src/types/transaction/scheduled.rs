use std::ops::Range;

use crate::error::Result;
use crate::types::transaction::PriorityFeeTransaction;

use super::rlp_utils::{rlp_decode_offsets, rlp_decode_opt_address, rlp_decode_u256};
use super::{Address, Transaction, TransactionType};
use alloy_rlp::{Decodable as RlpDecodable, Header as RlpHeader};
use ethnum::U256;
use solana_program::keccak;

pub struct Scheduled<T>
where
    T: AsRef<[u8]>,
{
    rlp: T,
    hash: keccak::Hash,

    payer: Address,
    sender: Option<Address>,

    nonce: u64,
    index: u16,

    intent: Option<Address>,
    intent_call_data: Range<usize>,

    target: Option<Address>,
    call_data: Range<usize>,

    value: U256,
    chain_id: u64,

    gas_limit: U256,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
}

impl<T: AsRef<[u8]>> Scheduled<T> {
    pub fn decode(encoded_transaction: T, hash: keccak::Hash) -> Result<Self> {
        let data = encoded_transaction.as_ref();
        let mut buffer = data;

        let mut rlp = RlpHeader::decode_bytes(&mut buffer, true)?;
        if !buffer.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        let payer = RlpDecodable::decode(&mut rlp)?;
        let sender = rlp_decode_opt_address(&mut rlp)?;

        let nonce = RlpDecodable::decode(&mut rlp)?;
        let index = RlpDecodable::decode(&mut rlp)?;

        let intent = rlp_decode_opt_address(&mut rlp)?;
        let intent_call_data = rlp_decode_offsets(&mut rlp, data)?;

        let target = rlp_decode_opt_address(&mut rlp)?;

        let call_data = rlp_decode_offsets(&mut rlp, data)?;

        let value = rlp_decode_u256(&mut rlp)?;
        let chain_id = RlpDecodable::decode(&mut rlp)?;

        let gas_limit = rlp_decode_u256(&mut rlp)?;
        let max_fee_per_gas = rlp_decode_u256(&mut rlp)?;
        let max_priority_fee_per_gas = rlp_decode_u256(&mut rlp)?;

        if !rlp.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        Ok(Self {
            rlp: encoded_transaction,
            hash,
            payer,
            sender,
            nonce,
            index,
            intent,
            intent_call_data,
            target,
            call_data,
            value,
            chain_id,
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
        })
    }
}

impl<T: AsRef<[u8]>> Transaction for Scheduled<T> {
    fn transaction_type(&self) -> TransactionType {
        TransactionType::Scheduled
    }

    fn hash(&self) -> &[u8; 32] {
        &self.hash.0
    }

    fn chain_id(&self) -> Option<u64> {
        Some(self.chain_id)
    }

    fn nonce(&self) -> u64 {
        self.nonce
    }

    fn gas_limit(&self) -> U256 {
        self.gas_limit
    }

    fn gas_price(&self) -> U256 {
        self.max_fee_per_gas
    }

    fn target(&self) -> Option<&Address> {
        self.target.as_ref()
    }

    fn call_data(&self) -> &[u8] {
        let rlp = self.rlp.as_ref();
        &rlp[self.call_data.start..self.call_data.end]
    }

    fn value(&self) -> U256 {
        self.value
    }

    fn recover_caller_address(&self) -> Result<Address> {
        let caller = self.sender.unwrap_or(self.payer);
        Ok(caller)
    }

    fn as_scheduled(&self) -> Option<&dyn ScheduledTransaction> {
        Some(self)
    }

    fn as_priority_fee(&self) -> Option<&dyn PriorityFeeTransaction> {
        Some(self)
    }
}

impl<T: AsRef<[u8]>> PriorityFeeTransaction for Scheduled<T> {
    fn max_fee_per_gas(&self) -> U256 {
        self.max_fee_per_gas
    }

    fn max_priority_fee_per_gas(&self) -> U256 {
        self.max_priority_fee_per_gas
    }
}

pub trait ScheduledTransaction: PriorityFeeTransaction {
    fn payer(&self) -> &Address;
    fn sender(&self) -> Option<&Address>;
    fn index(&self) -> u16;
    fn intent(&self) -> Option<&Address>;
    fn intent_call_data(&self) -> &[u8];
}

impl<T: AsRef<[u8]>> ScheduledTransaction for Scheduled<T> {
    fn payer(&self) -> &Address {
        &self.payer
    }

    fn sender(&self) -> Option<&Address> {
        self.sender.as_ref()
    }

    fn index(&self) -> u16 {
        self.index
    }

    fn intent(&self) -> Option<&Address> {
        self.intent.as_ref()
    }

    fn intent_call_data(&self) -> &[u8] {
        let rlp = self.rlp.as_ref();
        &rlp[self.intent_call_data.start..self.intent_call_data.end]
    }
}
