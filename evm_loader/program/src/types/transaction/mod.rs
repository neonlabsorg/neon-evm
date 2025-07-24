#![allow(clippy::cast_sign_loss)] // todo rust 1.87.0 - remove

use ethnum::U256;
use solana_program::keccak;

use super::Address;
use crate::{
    account::TransactionTree,
    error::{Error, Result},
    platform::Platform,
};

mod eip1559;
mod eip2930;
mod legacy;
mod rlp_utils;
mod scheduled;

use eip1559::Eip1559;
use eip2930::Eip2930;
use legacy::Legacy;
use scheduled::Scheduled;

pub use eip1559::PriorityFeeTransaction;
use rlp_utils::VecOffset;
pub use scheduled::ScheduledTransaction;

pub enum EncodedTransaction<'a> {
    Owned(Vec<u8>, keccak::Hash),
    Ref(&'a [u8], keccak::Hash),
}

impl<'a> EncodedTransaction<'a> {
    #[must_use]
    pub fn from_rlp(rlp: &'a [u8]) -> Self {
        let hash = keccak::hash(rlp);
        EncodedTransaction::Ref(rlp, hash)
    }

    #[must_use]
    pub fn rlp_len(&self) -> usize {
        self.rlp().len()
    }

    #[must_use]
    pub fn rlp(&self) -> &[u8] {
        match self {
            EncodedTransaction::Owned(rlp, _) => rlp.as_ref(),
            EncodedTransaction::Ref(rlp, _) => rlp,
        }
    }

    #[must_use]
    pub fn hash(&self) -> &keccak::Hash {
        match self {
            Self::Owned(_, hash) | Self::Ref(_, hash) => hash,
        }
    }

    fn transaction_type_and_rlp_offset(&self) -> Result<(TransactionType, usize)> {
        let rlp = self.rlp();

        match rlp[0] {
            0x00 => Ok((TransactionType::Legacy, 1)),
            0x01 => Ok((TransactionType::EIP2930, 1)),
            0x02 => Ok((TransactionType::EIP1559, 1)),
            0x7f => {
                let subtype = rlp[1];
                if subtype == 0x01 {
                    Ok((TransactionType::Scheduled, 2))
                } else {
                    Err(Error::UnsupportedNeonTransactionType(subtype))
                }
            }
            byte if (byte >= alloy_rlp::EMPTY_LIST_CODE) => Ok((TransactionType::Legacy, 0)),
            byte => Err(Error::UnsupportedEthereumTransactionType(byte)),
        }
    }

    #[inline]
    fn decode_inner<T1, T2, F1, F2>(
        self,
        offset: usize,
        from_vec: F1,
        from_slice: F2,
    ) -> Result<Box<dyn Transaction + 'a>>
    where
        T1: Transaction + 'static,
        F1: FnOnce(VecOffset<u8>, keccak::Hash) -> Result<T1>,
        T2: Transaction + 'a,
        F2: FnOnce(&'a [u8], keccak::Hash) -> Result<T2>,
    {
        match self {
            EncodedTransaction::Owned(rlp, hash) => {
                let rlp = VecOffset::new(rlp, offset);

                let transaction = from_vec(rlp, hash)?;
                Ok(Box::new(transaction))
            }
            EncodedTransaction::Ref(rlp, hash) => {
                let rlp = &rlp[offset..];

                let transaction = from_slice(rlp, hash)?;
                Ok(Box::new(transaction))
            }
        }
    }

    pub fn decode(self) -> Result<Box<dyn Transaction + 'a>> {
        let (transaction_type, offset) = self.transaction_type_and_rlp_offset()?;

        match transaction_type {
            TransactionType::Legacy => self.decode_inner(
                offset,
                Legacy::<VecOffset<u8>>::decode,
                Legacy::<&[u8]>::decode,
            ),
            TransactionType::EIP2930 => self.decode_inner(
                offset,
                Eip2930::<VecOffset<u8>>::decode,
                Eip2930::<&[u8]>::decode,
            ),
            TransactionType::EIP1559 => self.decode_inner(
                offset,
                Eip1559::<VecOffset<u8>>::decode,
                Eip1559::<&[u8]>::decode,
            ),
            TransactionType::Scheduled => self.decode_inner(
                offset,
                Scheduled::<VecOffset<u8>>::decode,
                Scheduled::<&[u8]>::decode,
            ),
        }
    }
}

#[repr(u8)]
#[derive(PartialEq, Eq)]
pub enum TransactionType {
    Legacy = 0,
    EIP2930 = 1,
    EIP1559 = 2,
    Scheduled = 0x80,
}

pub trait Transaction {
    fn transaction_type(&self) -> TransactionType;
    fn hash(&self) -> &[u8; 32];

    fn chain_id(&self) -> Option<u64>;

    fn nonce(&self) -> u64;

    fn gas_limit(&self) -> U256;
    fn gas_price(&self) -> U256;

    fn target(&self) -> Option<&Address>;
    fn call_data(&self) -> &[u8];
    fn value(&self) -> U256;

    fn recover_caller_address(&self) -> Result<Address>;

    fn as_scheduled(&self) -> Option<&dyn ScheduledTransaction> {
        None
    }

    fn as_priority_fee(&self) -> Option<&dyn PriorityFeeTransaction> {
        None
    }

    fn is(&self, transaction_type: TransactionType) -> bool {
        self.transaction_type() == transaction_type
    }
}

#[maybe_async::maybe_async]
pub async fn validate_transaction<'a>(
    tx: &dyn Transaction,
    origin: Address,
    platform: &impl Platform<'a>,
    tree: Option<&TransactionTree<'_>>,
) -> Result<()> {
    let chain_id = tx.chain_id().unwrap_or_else(|| platform.default_chain());

    if !platform.chains().any(|c| c.id == chain_id) {
        return Err(Error::InvalidChainId(chain_id));
    }

    if tree.is_some() != tx.is(TransactionType::Scheduled) {
        return Err(Error::TreeAccountTxInvalidType);
    }

    if let Some(tx) = tx.as_priority_fee() {
        if tx.max_fee_per_gas() < tx.max_priority_fee_per_gas() {
            return Err("max_fee_per_gas < max_priority_fee_per_gas".into());
        }
    }

    // Nonce validation is slightly different for classic and scheduled transactions.
    //
    // Classic transactions:
    // origin's nonce should be equal to txn's nonce because it's validated during
    // the first iteration and then incremented.
    //
    // Scheduled transactions:
    // payer's nonce (origin) validated only for the first transaction in the tree
    let validate_nonce = tree.map_or(true, TransactionTree::is_not_started);
    if !validate_nonce {
        return Ok(());
    }

    let origin_nonce = platform
        .get_balance(origin, chain_id)
        .await?
        .map_or(0_u64, |a| a.nonce());

    if origin_nonce != tx.nonce() {
        let error = Error::InvalidTransactionNonce(origin, origin_nonce, tx.nonce());
        return Err(error);
    }

    Ok(())
}
