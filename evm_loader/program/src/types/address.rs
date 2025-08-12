use hex::FromHex;
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use std::convert::{From, TryInto};
use std::fmt::{Debug, Display};
use std::str::FromStr;

use crate::account::{pda, Operator};
use crate::error::Error;

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Address(pub [u8; 20]);

impl Address {
    #[inline]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    #[must_use]
    pub fn from_create(source: &Self, nonce: u64) -> Self {
        use solana_program::keccak::{hash, Hash};

        #[derive(alloy_rlp::RlpEncodable)]
        struct RlpSource<'a> {
            source: &'a Address,
            nonce: u64,
        }

        let rlp = alloy_rlp::encode(RlpSource { source, nonce });
        let Hash(hash) = hash(&rlp);

        let bytes = arrayref::array_ref![hash, 12, 20];
        Self(*bytes)
    }

    #[must_use]
    pub fn from_create2(source: &Self, salt: &[u8; 32], initialization_code: &[u8]) -> Self {
        use solana_program::keccak::{hash, hashv, Hash};

        let Hash(code_hash) = hash(initialization_code);
        let Hash(hash) = hashv(&[&[0xFF], source.as_bytes(), salt, &code_hash]);

        let bytes = arrayref::array_ref![hash, 12, 20];
        Self(*bytes)
    }

    #[must_use]
    pub fn from_solana_address(source: &Pubkey) -> Self {
        use solana_program::keccak::{hash, Hash};

        let Hash(hash) = hash(&source.to_bytes());

        let bytes = arrayref::array_ref![hash, 12, 20];
        Self(*bytes)
    }

    pub fn from_hex(mut s: &str) -> Result<Self, Error> {
        if s.starts_with("0x") {
            s = &s[2..];
        }

        let bytes = <[u8; 20]>::from_hex(s)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn find_solana_address(&self, program_id: &Pubkey) -> (Pubkey, u8) {
        pda::contract_address(program_id, self)
    }

    #[must_use]
    pub fn find_balance_address(&self, program_id: &Pubkey, chain_id: u64) -> (Pubkey, u8) {
        pda::balance_address(program_id, self, chain_id)
    }

    #[must_use]
    pub fn find_operator_address(
        &self,
        program_id: &Pubkey,
        chain_id: u64,
        operator: &Operator,
    ) -> (Pubkey, u8) {
        pda::operator_address(program_id, operator.key, self, chain_id)
    }
}

impl FromStr for Address {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl From<[u8; 20]> for Address {
    fn from(value: [u8; 20]) -> Self {
        Self(value)
    }
}

impl<'r> From<&'r [u8; 20]> for &'r Address {
    fn from(value: &'r [u8; 20]) -> Self {
        #[allow(clippy::transmute_ptr_to_ptr)]
        // https://github.com/rust-lang/rust-clippy/issues/6372
        unsafe {
            std::mem::transmute(value)
        }
    }
}

impl From<Address> for [u8; 20] {
    fn from(value: Address) -> Self {
        value.0
    }
}

impl<'r> From<&'r Address> for &'r [u8; 20] {
    fn from(value: &'r Address) -> Self {
        value.as_bytes()
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex_string = hex::encode(self.0);
        f.write_str("0x")?;
        f.write_str(&hex_string)
    }
}

impl Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex_string = hex::encode(self.0);
        f.write_str("0x")?;
        f.write_str(&hex_string)
    }
}

impl alloy_rlp::Encodable for Address {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let Self(bytes) = self;
        bytes.encode(out);
    }

    fn length(&self) -> usize {
        let Self(bytes) = self;
        bytes.length()
    }
}
alloy_rlp::impl_max_encoded_len!(Address, 20);

impl alloy_rlp::Decodable for Address {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let bytes = <[u8; 20]>::decode(buf)?;
        Ok(Self(bytes))
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            self.to_string().serialize(serializer)
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct AddressVisitor;

        impl serde::de::Visitor<'_> for AddressVisitor {
            type Value = Address;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("Ethereum Address")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let address = Address::from_hex(v)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(v), &self))?;

                Ok(address)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let bytes = v
                    .try_into()
                    .map_err(|_| E::invalid_length(v.len(), &self))?;

                Ok(Address(bytes))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_str(AddressVisitor)
        } else {
            deserializer.deserialize_bytes(AddressVisitor)
        }
    }
}
