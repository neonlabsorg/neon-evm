use ethnum::U256;
use std::convert::TryInto;

use crate::error::Error;

use super::Address;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StorageKey([u8; 32]);

impl rlp::Decodable for StorageKey {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        rlp.decoder().decode_value(|bytes| {
            let array: [u8; 32] = bytes
                .try_into()
                .map_err(|_| rlp::DecoderError::RlpInvalidLength)?;
            Ok(Self(array))
        })
    }
}

enum RlpTransactionEnvelope {
    Legacy,
    AccessList,
    DynamicFee,
    Blob,
}

impl RlpTransactionEnvelope {
    pub fn get_type(bytes: &[u8]) -> (RlpTransactionEnvelope, &[u8]) {
        // Legacy transaction format
        if rlp::Rlp::new(bytes).is_list() {
            (RlpTransactionEnvelope::Legacy, bytes)
        // It's an EIP-2718 typed TX envelope.
        } else {
            match bytes[0] {
                0x00 => (RlpTransactionEnvelope::Legacy, &bytes[1..]),
                0x01 => (RlpTransactionEnvelope::AccessList, &bytes[1..]),
                0x02 => (RlpTransactionEnvelope::DynamicFee, &bytes[1..]),
                0x03 => (RlpTransactionEnvelope::Blob, &bytes[1..]),
                byte => panic!("Unsupported EIP-2718 Transaction type | First byte: {byte}"),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacyTx {
    pub nonce: u64,
    pub gas_price: U256,
    pub gas_limit: U256,
    pub target: Option<Address>,
    pub value: U256,
    pub call_data: crate::evm::Buffer,
    pub v: U256,
    pub r: U256,
    pub s: U256,
    pub chain_id: Option<U256>,
    pub recovery_id: u8,
    pub rlp_len: usize,
    pub hash: [u8; 32],
    pub signed_hash: [u8; 32],
}

impl rlp::Decodable for LegacyTx {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
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

        let tx = LegacyTx {
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
}

#[derive(Debug, Clone)]
pub struct AccessListTx {
    nonce: u64,
    gas_price: U256,
    gas_limit: U256,
    target: Option<Address>,
    value: U256,
    call_data: crate::evm::Buffer,
    v: U256,
    r: U256,
    s: U256,
    chain_id: U256,
    recovery_id: u8,
    access_list: Vec<(Address, Vec<StorageKey>)>,
    rlp_len: usize,
    hash: [u8; 32],
    signed_hash: [u8; 32],
}

impl rlp::Decodable for AccessListTx {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let rlp_len = {
            let info = rlp.payload_info()?;
            info.header_len + info.value_len
        };

        if rlp.as_raw().len() != rlp_len {
            return Err(rlp::DecoderError::RlpInconsistentLengthAndData);
        }

        let chain_id: U256 = u256(&rlp.at(0)?)?;
        let nonce: u64 = rlp.val_at(1)?;
        let gas_price: U256 = u256(&rlp.at(2)?)?;
        let gas_limit: U256 = u256(&rlp.at(3)?)?;
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

        // Vec<(Address, Vec<Pubkey>)>
        let rlp_access_list = rlp.at(6)?;
        let mut access_list = vec![];

        for entry in rlp_access_list.iter() {
            // Check if entry is a list
            if entry.is_list() {
                // Parse address from first element
                let address: Address = entry.at(0)?.as_val()?;

                // Get storage keys from second element
                let mut storage_keys: Vec<StorageKey> = vec![];

                for key in entry.at(1)?.iter() {
                    storage_keys.push(key.as_val()?);
                }

                access_list.push((address, storage_keys));
            }
        }

        let v: U256 = u256(&rlp.at(7)?)?;
        let r: U256 = u256(&rlp.at(8)?)?;
        let s: U256 = u256(&rlp.at(9)?)?;

        if rlp.at(10).is_ok() {
            return Err(rlp::DecoderError::RlpIncorrectListLen);
        }

        let hash = solana_program::keccak::hash(rlp.as_raw()).to_bytes();
        let signed_hash = signed_hash(rlp, Some(chain_id))?;

        let tx = AccessListTx {
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
            recovery_id: 0,
            access_list,
            rlp_len,
            hash,
            signed_hash,
        };

        Ok(tx)
    }
}

// TODO: Will be added as a part of EIP-1559
// struct DynamicFeeTx {}

// TODO: Will be added as a part of EIP-1559
// struct BlobTx {}

#[derive(Debug, Clone)]
pub enum Transaction {
    Legacy(LegacyTx),
    AccessList(AccessListTx),
}

impl Transaction {
    pub fn from_rlp(transaction: &[u8]) -> Result<Self, Error> {
        let (transaction_type, transaction) = RlpTransactionEnvelope::get_type(transaction);

        let tx = match transaction_type {
            RlpTransactionEnvelope::Legacy => {
                Transaction::Legacy(rlp::decode::<LegacyTx>(transaction).map_err(Error::from)?)
            }
            RlpTransactionEnvelope::AccessList => Transaction::AccessList(
                rlp::decode::<AccessListTx>(transaction).map_err(Error::from)?,
            ),
            _ => unimplemented!(),
        };

        Ok(tx)
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
            Transaction::Legacy(LegacyTx { nonce, .. })
            | Transaction::AccessList(AccessListTx { nonce, .. }) => *nonce,
        }
    }

    #[must_use]
    pub fn gas_price(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { gas_price, .. })
            | Transaction::AccessList(AccessListTx { gas_price, .. }) => gas_price,
        }
    }

    #[must_use]
    pub fn gas_limit(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { gas_limit, .. })
            | Transaction::AccessList(AccessListTx { gas_limit, .. }) => gas_limit,
        }
    }

    #[must_use]
    pub fn target(&self) -> Option<&Address> {
        match self {
            Transaction::Legacy(LegacyTx { target, .. })
            | Transaction::AccessList(AccessListTx { target, .. }) => target.as_ref(),
        }
    }

    #[must_use]
    pub fn value(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { value, .. })
            | Transaction::AccessList(AccessListTx { value, .. }) => value,
        }
    }

    #[must_use]
    pub fn call_data(&self) -> &crate::evm::Buffer {
        match self {
            Transaction::Legacy(LegacyTx { call_data, .. })
            | Transaction::AccessList(AccessListTx { call_data, .. }) => call_data,
        }
    }

    #[must_use]
    pub fn v(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { v, .. })
            | Transaction::AccessList(AccessListTx { v, .. }) => v,
        }
    }

    #[must_use]
    pub fn r(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { r, .. })
            | Transaction::AccessList(AccessListTx { r, .. }) => r,
        }
    }

    #[must_use]
    pub fn s(&self) -> &U256 {
        match self {
            Transaction::Legacy(LegacyTx { s, .. })
            | Transaction::AccessList(AccessListTx { s, .. }) => s,
        }
    }

    #[must_use]
    pub fn chain_id(&self) -> Option<&U256> {
        match self {
            Transaction::Legacy(LegacyTx { chain_id, .. }) => chain_id.as_ref(),
            Transaction::AccessList(AccessListTx { chain_id, .. }) => Some(chain_id),
        }
    }

    #[must_use]
    pub fn recovery_id(&self) -> u8 {
        match self {
            Transaction::Legacy(LegacyTx { recovery_id, .. })
            | Transaction::AccessList(AccessListTx { recovery_id, .. }) => *recovery_id,
        }
    }

    #[must_use]
    pub fn rlp_len(&self) -> usize {
        match self {
            Transaction::Legacy(LegacyTx { rlp_len, .. })
            | Transaction::AccessList(AccessListTx { rlp_len, .. }) => *rlp_len,
        }
    }

    #[must_use]
    pub fn hash(&self) -> &[u8; 32] {
        match self {
            Transaction::Legacy(LegacyTx { hash, .. })
            | Transaction::AccessList(AccessListTx { hash, .. }) => hash,
        }
    }

    #[must_use]
    pub fn signed_hash(&self) -> &[u8; 32] {
        match self {
            Transaction::Legacy(LegacyTx { signed_hash, .. })
            | Transaction::AccessList(AccessListTx { signed_hash, .. }) => signed_hash,
        }
    }

    #[must_use]
    pub fn access_list(&self) -> Option<&Vec<(Address, Vec<StorageKey>)>> {
        match self {
            Transaction::AccessList(AccessListTx { access_list, .. }) => Some(access_list),
            Transaction::Legacy(_) => None,
        }
    }
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
