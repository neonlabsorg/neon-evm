use std::ops::Range;

use crate::error::{Error, Result};

use super::rlp_utils::{
    concat_signature, rlp_decode_array, rlp_decode_offsets, rlp_decode_opt_address, rlp_decode_u256,
};
use super::{Address, Transaction, TransactionType};
use alloy_rlp::{Decodable as RlpDecodable, Encodable as RlpEncodable, Header as RlpHeader};
use ethnum::U256;
use solana_program::keccak;

pub struct Legacy<T>
where
    T: AsRef<[u8]>,
{
    rlp: T,
    hash: keccak::Hash,
    signed_hash: keccak::Hash,

    nonce: u64,
    gas_price: U256,
    gas_limit: U256,
    target: Option<Address>,
    value: U256,
    call_data: Range<usize>,

    v: U256,
    r: [u8; 32],
    s: [u8; 32],
}

impl<T: AsRef<[u8]>> Legacy<T> {
    pub fn decode(encoded_transaction: T, hash: keccak::Hash) -> Result<Self> {
        let data = encoded_transaction.as_ref();
        let mut buffer = data;

        let mut rlp = RlpHeader::decode_bytes(&mut buffer, true)?;
        if !buffer.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        let signed_start = rlp.as_ptr();

        let nonce = RlpDecodable::decode(&mut rlp)?;
        let gas_price = rlp_decode_u256(&mut rlp)?;
        let gas_limit = rlp_decode_u256(&mut rlp)?;
        let target = rlp_decode_opt_address(&mut rlp)?;
        let value = rlp_decode_u256(&mut rlp)?;

        let call_data = rlp_decode_offsets(&mut rlp, data)?;

        let signed_end = rlp.as_ptr();
        let signed_len = unsafe { signed_end.offset_from(signed_start) as usize }; // todo rust 1.87.0 offset_from_unsigned

        let v = rlp_decode_u256(&mut rlp)?;
        let r = rlp_decode_array(&mut rlp)?;
        let s = rlp_decode_array(&mut rlp)?;

        if !rlp.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength.into());
        }

        let signed_hash = {
            let signed_data = unsafe { std::slice::from_raw_parts(signed_start, signed_len) };
            let chain_id = calculate_chain_id(v);

            calculate_signed_hash(signed_data, chain_id)
        };

        Ok(Self {
            rlp: encoded_transaction,
            hash,
            signed_hash,
            nonce,
            gas_price,
            gas_limit,
            target,
            value,
            call_data,
            v,
            r,
            s,
        })
    }
}

impl<T: AsRef<[u8]>> Transaction for Legacy<T> {
    fn transaction_type(&self) -> TransactionType {
        TransactionType::Legacy
    }

    fn hash(&self) -> &[u8; 32] {
        &self.hash.0
    }

    fn chain_id(&self) -> Option<u64> {
        calculate_chain_id(self.v)
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
        let recovery_id = calculate_recovery_id(self.v)?;
        let signature = concat_signature(&self.r, &self.s);

        let public_key = secp256k1_recover(signed_hash, recovery_id, &signature)?;

        let keccak::Hash(address) = keccak::hash(&public_key.to_bytes());
        let address: [u8; 20] = address[12..32].try_into()?;

        Ok(Address::from(address))
    }
}

#[inline]
fn calculate_chain_id(v: U256) -> Option<u64> {
    if v >= 35 {
        let chain_id = (v - 1) / 2 - 17;
        let chain_id = chain_id.try_into().expect("chain_id < u64::max");

        Some(chain_id)
    } else {
        None
    }
}

#[inline]
fn calculate_recovery_id(v: U256) -> Result<u8> {
    if v >= 35 {
        Ok(u8::from((v % 2) == U256::ZERO))
    } else if v == 27 {
        Ok(0)
    } else if v == 28 {
        Ok(1)
    } else {
        Err(Error::Custom("Invalid recovery id".into()))
    }
}

fn calculate_signed_hash(signed_data: &[u8], chain_id: Option<u64>) -> keccak::Hash {
    let tail = chain_id.map_or_else(Vec::new, |chain_id| {
        let mut tail = Vec::with_capacity(1 + 8 + 2);
        RlpEncodable::encode(&chain_id, &mut tail);

        tail.extend_from_slice(&[0x80, 0x80]);
        tail
    });

    let header: Vec<u8> = {
        let mut header = Vec::new();
        RlpHeader {
            payload_length: signed_data.len() + tail.len(),
            list: true,
        }
        .encode(&mut header);

        header
    };

    keccak::hashv(&[&header, signed_data, &tail])
}
