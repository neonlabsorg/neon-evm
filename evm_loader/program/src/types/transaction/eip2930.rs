use std::ops::Range;

use crate::error::Result;

use super::rlp_utils::{
    concat_signature, rlp_decode_array, rlp_decode_offsets, rlp_decode_opt_address,
    rlp_decode_u256, rlp_list_header,
};
use super::{Address, Transaction, TransactionType};
use alloy_rlp::{Decodable as RlpDecodable, Header as RlpHeader};
use ethnum::U256;
use solana_program::keccak;

pub struct Eip2930<T>
where
    T: AsRef<[u8]>,
{
    rlp: T,
    hash: keccak::Hash,
    signed_hash: keccak::Hash,

    chain_id: u64,
    nonce: u64,
    gas_price: U256,
    gas_limit: U256,
    target: Option<Address>,
    value: U256,
    call_data: Range<usize>,

    recovery_id: u8,
    r: [u8; 32],
    s: [u8; 32],
    // access_list: Vec<_>
}

impl<T: AsRef<[u8]>> Eip2930<T> {
    pub fn decode(encoded_transaction: T, hash: keccak::Hash) -> Result<Self> {
        let data = encoded_transaction.as_ref();
        let mut buffer = data;

        let mut rlp = RlpHeader::decode_bytes(&mut buffer, true)?;
        if !buffer.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        let signed_start = rlp.as_ptr();

        let chain_id = RlpDecodable::decode(&mut rlp)?;
        let nonce = RlpDecodable::decode(&mut rlp)?;
        let gas_price = rlp_decode_u256(&mut rlp)?;
        let gas_limit = rlp_decode_u256(&mut rlp)?;
        let target = rlp_decode_opt_address(&mut rlp)?;
        let value = rlp_decode_u256(&mut rlp)?;

        let call_data = rlp_decode_offsets(&mut rlp, data)?;

        let _access_list = RlpHeader::decode_bytes(&mut rlp, true)?;

        let signed_end = rlp.as_ptr();
        let signed_len = unsafe { signed_end.offset_from(signed_start) as usize }; // todo rust 1.87.0 offset_from_unsigned

        let recovery_id = RlpDecodable::decode(&mut rlp)?;
        let r = rlp_decode_array(&mut rlp)?;
        let s = rlp_decode_array(&mut rlp)?;

        let signed_hash = {
            let signed_data = unsafe { std::slice::from_raw_parts(signed_start, signed_len) };
            calculate_signed_hash(signed_data)
        };

        if !rlp.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        Ok(Self {
            rlp: encoded_transaction,
            hash,
            signed_hash,
            chain_id,
            nonce,
            gas_price,
            gas_limit,
            target,
            value,
            call_data,
            recovery_id,
            r,
            s,
        })
    }
}

impl<T: AsRef<[u8]>> Transaction for Eip2930<T> {
    fn transaction_type(&self) -> TransactionType {
        TransactionType::EIP2930
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
        self.gas_price
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
        use solana_program::secp256k1_recover::secp256k1_recover;

        let keccak::Hash(signed_hash) = &self.signed_hash;
        let recovery_id = self.recovery_id;
        let signature = concat_signature(&self.r, &self.s);

        let public_key = secp256k1_recover(signed_hash, recovery_id, &signature)?;

        let keccak::Hash(address) = keccak::hash(&public_key.to_bytes());
        let address: [u8; 20] = address[12..32].try_into()?;

        Ok(Address::from(address))
    }
}

fn calculate_signed_hash(signed_data: &[u8]) -> keccak::Hash {
    let header = rlp_list_header(signed_data);

    keccak::hashv(&[&[0x01], &header, signed_data])
}
