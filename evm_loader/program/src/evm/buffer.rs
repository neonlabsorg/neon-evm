use std::ops::{Deref, Range};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

#[derive(Clone)]
pub enum Buffer {
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

impl Deref for Buffer {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Buffer::Owned(data) => data,
            Buffer::Account { range, data, .. } => unsafe {
                std::slice::from_raw_parts(data.add(range.start), range.len())
            },
            Buffer::AccountUninit { .. } => {
                panic!("attempted to dereference uninitialized account data")
            }
        }
    }
}

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

        Buffer::Account {
            key: *account.key,
            data,
            range,
        }
    }

    #[must_use]
    pub fn from_vec(v: Vec<u8>) -> Option<Self> {
        (!v.is_empty()).then_some(Buffer::Owned(v))
    }

    #[must_use]
    pub fn from_slice(v: &[u8]) -> Option<Self> {
        Self::from_vec(v.to_vec())
    }

    #[must_use]
    pub fn empty() -> Option<Self> {
        None
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        !matches!(self, Buffer::AccountUninit { .. })
    }

    #[must_use]
    pub fn uninit_data(&self) -> Option<(Pubkey, Range<usize>)> {
        if let Buffer::AccountUninit { key, range } = self {
            Some((*key, range.clone()))
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct OptionBuffer {
    // Sacrifice memory to store slice creation components to save discriminating the inner `Buffer`.
    ptr: *const u8,
    len: usize,
    inner: Option<Buffer>,
}

impl From<OptionBuffer> for Option<Buffer> {
    fn from(value: OptionBuffer) -> Self {
        value.inner
    }
}

impl From<Option<Buffer>> for OptionBuffer {
    fn from(inner: Option<Buffer>) -> Self {
        let (ptr, len) = match &inner {
            Some(Buffer::AccountUninit { .. }) => (core::ptr::null(), 0),
            Some(buffer) => (buffer.as_ptr(), buffer.len()),
            None => {
                let slice: &[u8] = Default::default();
                (slice.as_ptr(), slice.len())
            }
        };

        Self { ptr, len, inner }
    }
}

impl Default for OptionBuffer {
    fn default() -> Self {
        None.into()
    }
}

impl Deref for OptionBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        // We can not avoid this check because it is possible to create uninitialized account data,
        // but we should never read from it. Without this check, dereferencing `Option<Buffer>`
        // (with `buffer.as_deref().unwrap_or_default()`) and `OptionBuffer` will give different
        // results.
        //
        // TODO: It seems `OptionBuffer` is only used in `Machine`, if those buffers can't have
        // uninitialized account data, we may be able to create a different type that statically
        // guarantees that which would also allow us to remove this check (and the check in the
        // deserialization impls).
        assert!(
            !self.ptr.is_null(),
            "attempted to dereference uninitialized account data"
        );
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl OptionBuffer {
    pub const fn as_ref(&self) -> Option<&Buffer> {
        self.inner.as_ref()
    }

    pub fn as_deref(&self) -> Option<&[u8]> {
        self.inner.as_deref()
    }
}

impl serde::Serialize for OptionBuffer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStructVariant;

        match self.inner.as_ref() {
            None => serializer.serialize_unit_variant("evm_buffer", 0, "empty"),
            Some(Buffer::Owned(data)) => {
                let bytes = serde_bytes::Bytes::new(data);
                serializer.serialize_newtype_variant("evm_buffer", 1, "owned", bytes)
            }
            Some(Buffer::Account { key, range, .. }) => {
                let mut sv = serializer.serialize_struct_variant("evm_buffer", 2, "account", 2)?;
                sv.serialize_field("key", key)?;
                sv.serialize_field("range", range)?;
                sv.end()
            }
            Some(Buffer::AccountUninit { .. }) => {
                panic!("attempted to serialize uninitialized account data");
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for OptionBuffer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BufferVisitor;

        impl<'de> serde::de::Visitor<'de> for BufferVisitor {
            type Value = OptionBuffer;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("EVM Buffer")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Buffer::empty().into())
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Buffer::from_slice(v).into())
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
                Ok(Some(Buffer::AccountUninit { key, range }).into())
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::EnumAccess<'de>,
            {
                use serde::de::VariantAccess;

                let (index, variant) = data.variant::<u32>()?;
                match index {
                    0 => variant.unit_variant().map(|_| Buffer::empty().into()),
                    1 => variant
                        .newtype_variant()
                        .map(Buffer::from_slice)
                        .map(Into::into),
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
