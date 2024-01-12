use std::ops::{Deref, Range};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

#[derive(Clone)]
enum Inner {
    Owned(Vec<u8>),
    Account {
        key: Pubkey,
        range: Range<usize>,
        data: *const u8,
    },
    AccountUninit {
        key: Pubkey,
        range: Range<usize>,
    },
}

impl Deref for Inner {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Inner::Owned(data) => data,
            Inner::Account { range, data, .. } => unsafe {
                std::slice::from_raw_parts(data.add(range.start), range.len())
            },
            Inner::AccountUninit { .. } => {
                panic!("attempted to dereference uninitialized account data")
            }
        }
    }
}

#[derive(Clone)]
pub struct Buffer(Option<Inner>);

impl Buffer {
    /// # Safety
    ///
    /// This function was marked as unsafe until correct lifetimes will be set.
    /// At the moment, `Buffer` may outlive `account`, since no lifetimes has been set,
    /// so they are not checked by the compiler and it's the user's responsibility to take
    /// care of them.
    #[must_use]
    pub unsafe fn from_account(account: &AccountInfo, range: Range<usize>) -> Self {
        let data = unsafe {
            // todo cell_leak #69099
            let ptr = account.data.as_ptr();
            (*ptr).as_ptr()
        };

        Buffer(Some(Inner::Account {
            key: *account.key,
            data,
            range,
        }))
    }

    #[must_use]
    pub fn from_vec(v: Vec<u8>) -> Self {
        Buffer((!v.is_empty()).then_some(Inner::Owned(v)))
    }

    #[must_use]
    pub fn from_slice(v: &[u8]) -> Self {
        Self::from_vec(v.to_vec())
    }

    #[must_use]
    pub fn empty() -> Self {
        Buffer(None)
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.0
            .as_ref()
            .map(|inner| !matches!(inner, Inner::AccountUninit { .. }))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn uninit_data(&self) -> Option<(Pubkey, Range<usize>)> {
        if let Some(Inner::AccountUninit { key, range }) = self.0.as_ref() {
            Some((*key, range.clone()))
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn get_or_default(&self, index: usize) -> u8 {
        self.0
            .as_deref()
            .expect("attempted to index uninitialized account data")
            .get(index)
            .copied()
            .unwrap_or_default()
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
            .as_deref()
            .expect("attempted to dereference None buffer")
    }
}

impl From<Buffer> for Option<Inner> {
    fn from(value: Buffer) -> Self {
        value.0
    }
}

impl From<Option<Inner>> for Buffer {
    fn from(value: Option<Inner>) -> Self {
        Buffer(value)
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::empty()
    }
}

impl serde::Serialize for Buffer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStructVariant;

        match self.0.as_ref() {
            None => serializer.serialize_unit_variant("evm_buffer", 0, "empty"),
            Some(Inner::Owned(data)) => {
                let bytes = serde_bytes::Bytes::new(data);
                serializer.serialize_newtype_variant("evm_buffer", 1, "owned", bytes)
            }
            Some(Inner::Account { key, range, .. }) => {
                let mut sv = serializer.serialize_struct_variant("evm_buffer", 2, "account", 2)?;
                sv.serialize_field("key", key)?;
                sv.serialize_field("range", range)?;
                sv.end()
            }
            Some(Inner::AccountUninit { .. }) => {
                panic!("attempted to serialize uninitialized account data");
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Buffer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BufferVisitor;

        impl<'de> serde::de::Visitor<'de> for BufferVisitor {
            type Value = Buffer;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("EVM Buffer")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Buffer::empty())
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Buffer::from_slice(v))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let key = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let range = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                Ok(Buffer(Some(Inner::AccountUninit { key, range })))
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::EnumAccess<'de>,
            {
                use serde::de::VariantAccess;

                let (index, variant) = data.variant::<u32>()?;
                match index {
                    0 => variant.unit_variant().map(|_| Buffer::empty()),
                    1 => variant.newtype_variant().map(Buffer::from_slice),
                    2 => variant.struct_variant(&["key", "range"], self),
                    _ => Err(serde::de::Error::unknown_variant(
                        "_",
                        &["empty", "owned", "account"],
                    )),
                }
            }
        }

        deserializer.deserialize_enum("evm_buffer", &["empty", "owned", "account"], BufferVisitor)
    }
}
