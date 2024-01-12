use std::ops::{Deref, Range};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

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

pub struct Buffer {
    // We maintain a ptr and len to be able to construct a slice without having to discriminate
    // inner. This means we should not allow mutation of inner after the construction of a buffer.
    ptr: *const u8,
    len: usize,
    inner: Inner,
}

impl Buffer {
    fn new(inner: Inner) -> Self {
        let (ptr, len) = match &inner {
            Inner::Owned(data) => (data.as_ptr(), data.len()),
            Inner::Account { data, range, .. } => {
                let ptr = unsafe { data.add(range.start) };
                (ptr, range.len())
            }
            Inner::AccountUninit { .. } => (std::ptr::null(), 0),
        };

        Buffer { ptr, len, inner }
    }

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

        Buffer::new(Inner::Account {
            key: *account.key,
            data,
            range,
        })
    }

    #[must_use]
    pub fn from_vec(v: Vec<u8>) -> Self {
        Self::new(Inner::Owned(v))
    }

    #[must_use]
    pub fn from_slice(v: &[u8]) -> Self {
        Self::from_vec(v.to_vec())
    }

    #[must_use]
    pub fn empty() -> Self {
        Buffer::new(Inner::Owned(Vec::default()))
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        !matches!(self.inner, Inner::AccountUninit { .. })
    }

    #[must_use]
    pub fn uninit_data(&self) -> Option<(Pubkey, Range<usize>)> {
        if let Inner::AccountUninit { key, range } = &self.inner {
            Some((*key, range.clone()))
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn get_or_default(&self, index: usize) -> u8 {
        debug_assert!(!self.ptr.is_null());

        if index < self.len {
            unsafe { self.ptr.add(index).read() }
        } else {
            0
        }
    }
}

impl Deref for Buffer {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        debug_assert!(!self.ptr.is_null());

        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Clone for Buffer {
    #[inline]
    fn clone(&self) -> Self {
        match &self.inner {
            Inner::Owned { .. } => Self::from_slice(self),
            Inner::Account { key, data, range } => Self::new(Inner::Account {
                key: *key,
                range: range.clone(),
                data: *data,
            }),
            Inner::AccountUninit { key, range } => Self::new(Inner::AccountUninit {
                key: *key,
                range: range.clone(),
            }),
        }
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

        match &self.inner {
            Inner::Owned(data) => {
                // For backwards compatibility we need to serialize empty and non-empty vecs differently.
                if data.is_empty() {
                    serializer.serialize_unit_variant("evm_buffer", 0, "empty")
                } else {
                    let bytes = serde_bytes::Bytes::new(data);
                    serializer.serialize_newtype_variant("evm_buffer", 1, "owned", bytes)
                }
            }
            Inner::Account { key, range, .. } => {
                let mut sv = serializer.serialize_struct_variant("evm_buffer", 2, "account", 2)?;
                sv.serialize_field("key", key)?;
                sv.serialize_field("range", range)?;
                sv.end()
            }
            Inner::AccountUninit { .. } => {
                unreachable!()
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
                Ok(Buffer::new(Inner::AccountUninit { key, range }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_deref_owned_empty() {
        let data = Vec::default();
        let (ptr, len) = (data.as_ptr(), data.len());
        let slice = &*Buffer::default();
        assert_eq!(slice.as_ptr(), ptr);
        assert_eq!(slice.len(), len);
    }

    #[test]
    fn test_deref_owned_non_empty() {
        let data = vec![1];
        let (ptr, len) = (data.as_ptr(), data.len());
        let slice = &*Buffer::from_vec(data);
        assert_eq!(slice.as_ptr(), ptr);
        assert_eq!(slice.len(), len);
    }

    struct OwnedAccountInfo {
        key: Pubkey,
        lamports: u64,
        data: Vec<u8>,
        owner: Pubkey,
        rent_epoch: u64,
        is_signer: bool,
        is_writable: bool,
        executable: bool,
    }

    impl OwnedAccountInfo {
        fn with_data(data: Vec<u8>) -> Self {
            OwnedAccountInfo {
                key: Pubkey::default(),
                lamports: 0,
                data,
                owner: Pubkey::default(),
                rent_epoch: 0,
                is_signer: false,
                is_writable: false,
                executable: false,
            }
        }

        fn as_mut(&mut self) -> AccountInfo<'_> {
            AccountInfo {
                key: &self.key,
                lamports: Rc::new(RefCell::new(&mut self.lamports)),
                data: Rc::new(RefCell::new(&mut self.data)),
                owner: &self.owner,
                rent_epoch: self.rent_epoch,
                is_signer: self.is_signer,
                is_writable: self.is_writable,
                executable: self.executable,
            }
        }
    }

    #[test]
    fn test_deref_account_empty() {
        let data = Vec::default();
        let (ptr, len) = (data.as_ptr(), data.len());
        let mut account_info = OwnedAccountInfo::with_data(data);
        let slice = &*unsafe { Buffer::from_account(&account_info.as_mut(), 0..len) };
        assert_eq!(slice.as_ptr(), ptr);
        assert_eq!(slice.len(), len);
    }

    #[test]
    fn test_deref_account_non_empty() {
        let data = vec![1];
        let (ptr, len) = (data.as_ptr(), data.len());
        let mut account_info = OwnedAccountInfo::with_data(data);
        let slice = &*unsafe { Buffer::from_account(&account_info.as_mut(), 0..len) };
        assert_eq!(slice.as_ptr(), ptr);
        assert_eq!(slice.len(), len);
    }

    #[test]
    #[should_panic(expected = "assertion failed: !self.ptr.is_null()")]
    fn test_deref_account_uninit() {
        let _: &[u8] = &Buffer::new(Inner::AccountUninit {
            key: Pubkey::default(),
            range: 0..0,
        });
    }
}
