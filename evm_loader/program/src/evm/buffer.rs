use std::ops::{Deref, Range};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::types::vector::VectorSliceExt;
use crate::types::Vector;
use crate::vector;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[repr(C)]
enum Inner {
    Owned(Vector<u8>),
    Account {
        key: Pubkey,
        range: Range<usize>,
        data: *const u8,
    },
}

#[cfg_attr(test, derive(Debug))]
#[repr(C)]
pub struct Buffer {
    // We maintain a ptr and len to be able to construct a slice without having to discriminate
    // inner. This means we should not allow mutation of inner after the construction of a buffer.
    ptr: *const u8,
    len: usize,
    inner: Inner,
}

#[cfg(test)]
impl core::cmp::PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Buffer {
    fn new(inner: Inner) -> Self {
        let (ptr, len) = match &inner {
            Inner::Owned(data) => (data.as_ptr(), data.len()),
            Inner::Account { data, range, .. } => {
                let ptr = unsafe { data.add(range.start) };
                (ptr, range.len())
            }
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
    pub fn from_vector(data: Vector<u8>) -> Self {
        Self::new(Inner::Owned(data))
    }

    // todo check usages
    #[must_use]
    pub fn from_slice(v: &[u8]) -> Self {
        Self::from_vector(v.to_vector())
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::from_vector(vector![])
    }

    #[must_use]
    pub fn uninit_data(&self) -> Option<(Pubkey, Range<usize>)> {
        if let Inner::Account { key, range, .. } = &self.inner {
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
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{executor::OwnedAccountInfo, vector};
    use solana_program::account_info::IntoAccountInfo;

    macro_rules! assert_slice_ptr_eq {
        ($actual:expr, $expected:expr) => {{
            let actual: &[_] = $actual;
            let (expected_ptr, expected_len): (*const _, usize) = $expected;
            assert_eq!(actual.as_ptr(), expected_ptr);
            assert_eq!(actual.len(), expected_len);
        }};
    }

    #[test]
    fn test_deref_owned_empty() {
        let data = vector![];
        let expected = (data.as_ptr(), data.len());
        assert_slice_ptr_eq!(&*Buffer::from_vector(data), expected);
    }

    #[test]
    fn test_deref_owned_non_empty() {
        let data = vector![1];
        let expected = (data.as_ptr(), data.len());
        assert_slice_ptr_eq!(&*Buffer::from_vector(data), expected);
    }

    impl OwnedAccountInfo {
        fn with_data(data: Vector<u8>) -> Self {
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
    }

    #[test]
    fn test_deref_account_empty() {
        let data = vector![];
        let expected = (data.as_ptr(), data.len());
        let mut account_info = OwnedAccountInfo::with_data(data);
        assert_slice_ptr_eq!(
            &*unsafe { Buffer::from_account(&account_info.into_account_info(), 0..expected.1) },
            expected
        );
    }

    #[test]
    fn test_deref_account_non_empty() {
        let data = vector![1];
        let expected = (data.as_ptr(), data.len());
        let mut account_info = OwnedAccountInfo::with_data(data);
        assert_slice_ptr_eq!(
            &*unsafe { Buffer::from_account(&account_info.into_account_info(), 0..expected.1) },
            expected
        );
    }
}
