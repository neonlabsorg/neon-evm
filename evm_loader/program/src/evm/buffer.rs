use std::{
    ops::{Deref, Range},
    rc::Rc, cell::RefCell,
};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

#[derive(Clone)]
enum BufferInner<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a [u8]),
    Account {
        key: Pubkey,
        range: Range<usize>,
        // None when the account is uninitialized.
        account: Option<Rc<RefCell<&'a mut [u8]>>>,
    },
}

impl<'a> Deref for BufferInner<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match *self {
            BufferInner::Owned(ref data) => data,
            BufferInner::Borrowed(data) => data,
            BufferInner::Account { key, range, account } => {
                &*account.unwrap().borrow()
            }
        }
    }
}

#[derive(Clone)]
pub struct Buffer<'a>(Option<BufferInner<'a>>);

impl<'a> Buffer<'a> {
    #[must_use]
    pub fn from_account(account: &AccountInfo, range: Range<usize>) -> Self {
        Self(Some(BufferInner::Account {
            key: *account.key,
            range,
            account: Some(Rc::clone(&account.data)),
        }))
    }

    #[must_use]
    pub fn from_vec(v: Vec<u8>) -> Self {
        Self((!v.is_empty()).then_some(BufferInner::Owned(v)))
    }

    #[must_use]
    pub fn from_slice(v: &[u8]) -> Self {
        Self((!v.is_empty()).then_some(BufferInner::Borrowed(v)))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self(None)
    }

    #[must_use]
    pub fn buffer_is_empty(&self) -> bool {
        self.0.is_none()
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.0.map(|inner| !matches!(inner, BufferInner::Account { account: None, .. })).unwrap_or_default()
    }

    #[must_use]
    pub fn uninit_data(&self) -> Option<(Pubkey, Range<usize>)> {
        if let Some(BufferInner::Account { key, range, account: None }) = self.0 {
            Some((key, range))
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn get_or_default(&self, index: usize) -> u8 {
        self.0.as_deref().unwrap().get(0).copied().unwrap_or_default()
    }
}

impl<'a> Deref for Buffer<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.as_deref().unwrap()
    }
}

impl<'a> Default for Buffer<'a> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'a> serde::Serialize for Buffer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStructVariant;

        match &self.0 {
            None => serializer.serialize_unit_variant("evm_buffer", 0, "empty"),
            Some(BufferInner::Owned(data)) => {
                let bytes = serde_bytes::Bytes::new(data);
                serializer.serialize_newtype_variant("evm_buffer", 1, "owned", bytes)
            }
            Some(BufferInner::Borrowed(data)) => {
                let bytes = serde_bytes::Bytes::new(data);
                serializer.serialize_newtype_variant("evm_buffer", 1, "owned", bytes)
            }
            Some(BufferInner::Account { key, range, account }) => {
                account.unwrap_or_else(|| unreachable!());
                let mut sv = serializer.serialize_struct_variant("evm_buffer", 2, "account", 2)?;
                sv.serialize_field("key", &key)?;
                sv.serialize_field("range", &range)?;
                sv.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Buffer<'static> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BufferVisitor;

        impl<'de> serde::de::Visitor<'de> for BufferVisitor {
            type Value = Buffer<'static>;

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
                Ok(Buffer(Some(BufferInner::Account { key, range, account: None })))
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
