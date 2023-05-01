use std::array::TryFromSliceError;

use ethnum::U256;
use rlp::{DecoderError, Rlp};

use crate::types::{Address, Transaction, u256};

#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub parent_hash: U256,
    pub uncles_hash: U256,
    pub author: Address,
    pub state_root: U256,
    pub transactions_root: U256,
    pub receipts_root: U256,
    pub log_bloom: [u8; 256],
    pub difficulty: U256,
    pub number: u64,
    pub gas_limit: U256,
    pub gas_used: U256,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
}

impl rlp::Decodable for BlockHeader {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        Ok(Self {
            parent_hash: u256(&rlp.at(0)?)?,
            uncles_hash: u256(&rlp.at(1)?)?,
            author: rlp.val_at(2)?,
            state_root: u256(&rlp.at(3)?)?,
            transactions_root: u256(&rlp.at(4)?)?,
            receipts_root: u256(&rlp.at(5)?)?,
            log_bloom: rlp.at(6)?
                .data()?
                .try_into()
                .map_err(|_err: TryFromSliceError| DecoderError::Custom("Error converting log bloom to bytes"))?,
            difficulty: u256(&rlp.at(7)?)?,
            number: rlp.val_at(8)?,
            gas_limit: u256(&rlp.at(9)?)?,
            gas_used: u256(&rlp.at(10)?)?,
            timestamp: rlp.val_at(11)?,
            extra_data: rlp.val_at(12)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub unkles: Vec<BlockHeader>,
}

impl rlp::Decodable for Block {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        Ok(Self {
            header: rlp.val_at(0)?,
            transactions: rlp.list_at(1)?,
            unkles: rlp.list_at(2)?,
        })
    }
}