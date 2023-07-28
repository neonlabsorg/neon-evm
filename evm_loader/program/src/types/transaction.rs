use ethnum::U256;
use rlp::Decodable;
use std::convert::TryInto;

use crate::error::Error;

use super::Address;

enum RlpTransactionEnvelope {
    Legacy,
    AccessList,
    DynamicFee,
    Blob,
}

impl RlpTransactionEnvelope {
    pub fn from_rlp(rlp: &rlp::Rlp) -> RlpTransactionEnvelope {
        RlpTransactionEnvelope::decode(rlp).expect("RlpTransactionEnvelope decoding never fails")
    }
}

impl rlp::Decodable for RlpTransactionEnvelope {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        // Legacy transaction format
        if rlp.is_list() {
            return Ok(RlpTransactionEnvelope::Legacy);
        }

        let raw = rlp.as_raw();
        let first_byte = raw[0];

        // It's an EIP-2718 typed TX envelope.
        match first_byte {
            0x00 => Ok(RlpTransactionEnvelope::Legacy),
            0x01 => Ok(RlpTransactionEnvelope::AccessList),
            0x02 => Ok(RlpTransactionEnvelope::DynamicFee),
            0x03 => Ok(RlpTransactionEnvelope::Blob),
            byte => panic!("Unsupported EIP-2718 Transaction type | First byte: {byte}"),
        }
    }
}

// type AccAddress = [u8; 20];
// type Bytes32 = [u8; 32];

// Raven: we should switch this to enum
#[derive(Debug, Clone)]
pub enum Transaction {
    Legacy {
        nonce: u64,
        gas_price: U256,
        gas_limit: U256,
        target: Option<Address>,
        value: U256,
        call_data: crate::evm::Buffer,
        v: U256,
        r: U256,
        s: U256,
        chain_id: Option<U256>,
        recovery_id: u8,
        rlp_len: usize,
        hash: [u8; 32],
        signed_hash: [u8; 32],
    },
    AccessList {
        nonce: u64,
        gas_price: U256,
        gas_limit: U256,
        target: Option<Address>,
        value: U256,
        call_data: crate::evm::Buffer,
        v: U256,
        r: U256,
        s: U256,
        chain_id: Option<U256>,
        recovery_id: u8,
        // access_list: Vec<Vec<(AccAddress, Bytes32)>>,
        rlp_len: usize,
        hash: [u8; 32],
        signed_hash: [u8; 32],
    },
}

impl Transaction {
    pub fn from_rlp(transaction: &[u8]) -> Result<Self, Error> {
        rlp::decode(transaction).map_err(Error::from)
    }

    pub fn recover_caller_address(&self) -> Result<Address, Error> {
        use solana_program::keccak::{hash, Hash};
        use solana_program::secp256k1_recover::secp256k1_recover;

        let signature = [self.r().to_be_bytes(), self.s().to_be_bytes()].concat();
        let public_key = secp256k1_recover(self.signed_hash(), self.recovery_id(), &signature)?;

        let Hash(address) = hash(&public_key.to_bytes());
        let address: [u8; 20] = address[12..32].try_into()?;

        Ok(Address::from(address))
    }

    #[must_use]
    pub fn nonce(&self) -> u64 {
        match self {
            Transaction::Legacy { nonce, .. } | Transaction::AccessList { nonce, .. } => *nonce,
        }
    }

    #[must_use]
    pub fn gas_price(&self) -> &U256 {
        match self {
            Transaction::Legacy { gas_price, .. } | Transaction::AccessList { gas_price, .. } => {
                gas_price
            }
        }
    }

    #[must_use]
    pub fn gas_limit(&self) -> &U256 {
        match self {
            Transaction::Legacy { gas_limit, .. } | Transaction::AccessList { gas_limit, .. } => {
                gas_limit
            }
        }
    }

    #[must_use]
    pub fn target(&self) -> Option<&Address> {
        match self {
            Transaction::Legacy { target, .. } | Transaction::AccessList { target, .. } => {
                target.as_ref()
            }
        }
    }

    #[must_use]
    pub fn value(&self) -> &U256 {
        match self {
            Transaction::Legacy { value, .. } | Transaction::AccessList { value, .. } => value,
        }
    }

    #[must_use]
    pub fn call_data(&self) -> &crate::evm::Buffer {
        match self {
            Transaction::Legacy { call_data, .. } | Transaction::AccessList { call_data, .. } => {
                call_data
            }
        }
    }

    #[must_use]
    pub fn v(&self) -> &U256 {
        match self {
            Transaction::Legacy { v, .. } | Transaction::AccessList { v, .. } => v,
        }
    }

    #[must_use]
    pub fn r(&self) -> &U256 {
        match self {
            Transaction::Legacy { r, .. } | Transaction::AccessList { r, .. } => r,
        }
    }

    #[must_use]
    pub fn s(&self) -> &U256 {
        match self {
            Transaction::Legacy { s, .. } | Transaction::AccessList { s, .. } => s,
        }
    }

    #[must_use]
    pub fn chain_id(&self) -> Option<&U256> {
        match self {
            Transaction::Legacy { chain_id, .. } | Transaction::AccessList { chain_id, .. } => {
                chain_id.as_ref()
            }
        }
    }

    #[must_use]
    pub fn recovery_id(&self) -> u8 {
        match self {
            Transaction::Legacy { recovery_id, .. }
            | Transaction::AccessList { recovery_id, .. } => *recovery_id,
        }
    }

    #[must_use]
    pub fn rlp_len(&self) -> usize {
        match self {
            Transaction::Legacy { rlp_len, .. } | Transaction::AccessList { rlp_len, .. } => {
                *rlp_len
            }
        }
    }

    #[must_use]
    pub fn hash(&self) -> &[u8; 32] {
        match self {
            Transaction::Legacy { hash, .. } | Transaction::AccessList { hash, .. } => hash,
        }
    }

    #[must_use]
    pub fn signed_hash(&self) -> &[u8; 32] {
        match self {
            Transaction::Legacy { signed_hash, .. }
            | Transaction::AccessList { signed_hash, .. } => signed_hash,
        }
    }
}

impl rlp::Decodable for Transaction {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        match RlpTransactionEnvelope::from_rlp(rlp) {
            RlpTransactionEnvelope::Legacy => {
                let rlp_len = {
                    let info = rlp.payload_info()?;
                    info.header_len + info.value_len
                };

                if rlp.as_raw().len() != rlp_len {
                    return Err(rlp::DecoderError::RlpInconsistentLengthAndData);
                }

                let nonce: u64 = rlp.val_at(0)?;
                let gas_price: U256 = u256(&rlp.at(1)?)?;
                let gas_limit: U256 = u256(&rlp.at(2)?)?;
                let target: Option<Address> = {
                    let target = rlp.at(3)?;
                    if target.is_empty() {
                        if target.is_data() {
                            None
                        } else {
                            return Err(rlp::DecoderError::RlpExpectedToBeData);
                        }
                    } else {
                        Some(target.as_val()?)
                    }
                };
                let value: U256 = u256(&rlp.at(4)?)?;
                let call_data = crate::evm::Buffer::from_slice(rlp.at(5)?.data()?);
                let v: U256 = u256(&rlp.at(6)?)?;
                let r: U256 = u256(&rlp.at(7)?)?;
                let s: U256 = u256(&rlp.at(8)?)?;

                if rlp.at(9).is_ok() {
                    return Err(rlp::DecoderError::RlpIncorrectListLen);
                }

                let (chain_id, recovery_id) = if v >= 35 {
                    let chain_id = (v - 1) / 2 - 17;
                    let recovery_id = u8::from((v % 2) == U256::ZERO);
                    (Some(chain_id), recovery_id)
                } else if v == 27 {
                    (None, 0_u8)
                } else if v == 28 {
                    (None, 1_u8)
                } else {
                    return Err(rlp::DecoderError::RlpExpectedToBeData);
                };

                let hash = solana_program::keccak::hash(rlp.as_raw()).to_bytes();
                let signed_hash = signed_hash(rlp, chain_id)?;

                let tx = Transaction::Legacy {
                    nonce,
                    gas_price,
                    gas_limit,
                    target,
                    value,
                    call_data,
                    v,
                    r,
                    s,
                    chain_id,
                    recovery_id,
                    rlp_len,
                    hash,
                    signed_hash,
                };

                Ok(tx)
            }
            RlpTransactionEnvelope::AccessList => parse_access_list_rlp(rlp),
            RlpTransactionEnvelope::DynamicFee => {
                unimplemented!("Dynamic Fee Transaction is not supported");
            }
            RlpTransactionEnvelope::Blob => {
                unimplemented!("Blob Transaction is not supported");
            }
        }
    }
}

fn parse_access_list_rlp(rlp: &rlp::Rlp) -> Result<Transaction, rlp::DecoderError> {
    let rlp_len = {
        let info = rlp.payload_info()?;
        info.header_len + info.value_len
    };

    if rlp.as_raw().len() != rlp_len {
        return Err(rlp::DecoderError::RlpInconsistentLengthAndData);
    }

    let nonce: u64 = rlp.val_at(0)?;
    let gas_price: U256 = u256(&rlp.at(1)?)?;
    let gas_limit: U256 = u256(&rlp.at(2)?)?;
    let target: Option<Address> = {
        let target = rlp.at(3)?;
        if target.is_empty() {
            if target.is_data() {
                None
            } else {
                return Err(rlp::DecoderError::RlpExpectedToBeData);
            }
        } else {
            Some(target.as_val()?)
        }
    };
    let value: U256 = u256(&rlp.at(4)?)?;
    let call_data = crate::evm::Buffer::from_slice(rlp.at(5)?.data()?);
    let v: U256 = u256(&rlp.at(6)?)?;
    let r: U256 = u256(&rlp.at(7)?)?;
    let s: U256 = u256(&rlp.at(8)?)?;

    if rlp.at(9).is_ok() {
        return Err(rlp::DecoderError::RlpIncorrectListLen);
    }

    let (chain_id, recovery_id) = if v >= 35 {
        let chain_id = (v - 1) / 2 - 17;
        let recovery_id = u8::from((v % 2) == U256::ZERO);
        (Some(chain_id), recovery_id)
    } else if v == 27 {
        (None, 0_u8)
    } else if v == 28 {
        (None, 1_u8)
    } else {
        return Err(rlp::DecoderError::RlpExpectedToBeData);
    };

    let hash = solana_program::keccak::hash(rlp.as_raw()).to_bytes();
    let signed_hash = signed_hash(rlp, chain_id)?;

    let tx = Transaction::AccessList {
        nonce,
        gas_price,
        gas_limit,
        target,
        value,
        call_data,
        v,
        r,
        s,
        chain_id,
        recovery_id,
        rlp_len,
        hash,
        signed_hash,
    };

    Ok(tx)
}

fn signed_hash(
    transaction: &rlp::Rlp,
    chain_id: Option<U256>,
) -> Result<[u8; 32], rlp::DecoderError> {
    let raw = transaction.as_raw();
    let payload_info = transaction.payload_info()?;
    let (_, v_offset) = transaction.at_with_offset(6)?;

    let middle = &raw[payload_info.header_len..v_offset];

    let trailer = chain_id.map_or_else(Vec::new, |chain_id| {
        let chain_id = {
            let leading_empty_bytes = (chain_id.leading_zeros() as usize) / 8;
            let bytes = chain_id.to_be_bytes();
            bytes[leading_empty_bytes..].to_vec()
        };

        let mut trailer = Vec::with_capacity(64);
        match chain_id.len() {
            0 => {
                trailer.extend_from_slice(&[0x80]);
            }
            1 if chain_id[0] < 0x80 => {
                trailer.extend_from_slice(&chain_id);
            }
            len @ 1..=55 => {
                let len: u8 = len.try_into().unwrap();

                trailer.extend_from_slice(&[0x80 + len]);
                trailer.extend_from_slice(&chain_id);
            }
            _ => {
                unreachable!("chain_id.len() <= 32")
            }
        }

        trailer.extend_from_slice(&[0x80, 0x80]);
        trailer
    });

    let header: Vec<u8> = {
        let len = middle.len() + trailer.len();
        if len <= 55 {
            let len: u8 = len.try_into().unwrap();
            vec![0xC0 + len]
        } else {
            let len_bytes = {
                let leading_empty_bytes = (len.leading_zeros() as usize) / 8;
                let bytes = len.to_be_bytes();
                bytes[leading_empty_bytes..].to_vec()
            };
            let len_bytes_len: u8 = len_bytes.len().try_into().unwrap();

            let mut header = Vec::with_capacity(10);
            header.extend_from_slice(&[0xF7 + len_bytes_len]);
            header.extend_from_slice(&len_bytes);

            header
        }
    };

    let hash = solana_program::keccak::hashv(&[&header, middle, &trailer]).to_bytes();

    Ok(hash)
}

#[inline]
fn u256(rlp: &rlp::Rlp) -> Result<U256, rlp::DecoderError> {
    rlp.decoder().decode_value(|bytes| {
        if !bytes.is_empty() && bytes[0] == 0 {
            Err(rlp::DecoderError::RlpInvalidIndirection)
        } else if bytes.len() <= 32 {
            let mut buffer = [0_u8; 32];
            buffer[(32 - bytes.len())..].copy_from_slice(bytes);
            Ok(U256::from_be_bytes(buffer))
        } else {
            Err(rlp::DecoderError::RlpIsTooBig)
        }
    })
}
