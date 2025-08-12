use std::{mem::MaybeUninit, ops::Range};

use super::Address;
use crate::error::Result;
use ethnum::U256;

pub struct VecOffset<T> {
    data: Vec<T>,
    offset: usize,
}

impl<T> VecOffset<T> {
    #[inline]
    pub fn new(data: Vec<T>, offset: usize) -> Self {
        Self { data, offset }
    }
}

impl<T> AsRef<[T]> for VecOffset<T> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        &self.data[self.offset..]
    }
}

#[inline]
fn rlp_left_pad<const N: usize>(data: &[u8]) -> Result<[u8; N]> {
    if data.len() > N {
        return Err(alloy_rlp::Error::Overflow.into());
    }

    let mut v = [0; N];

    // yes, data may empty
    if data.is_empty() {
        return Ok(v);
    }

    if data[0] == 0 {
        return Err(alloy_rlp::Error::LeadingZero.into());
    }

    // SAFETY: length checked above
    unsafe { v.get_unchecked_mut(N - data.len()..) }.copy_from_slice(data);
    Ok(v)
}

#[inline]
pub fn rlp_decode_array<const N: usize>(buffer: &mut &[u8]) -> Result<[u8; N]> {
    let data: &[u8] = alloy_rlp::Header::decode_bytes(buffer, false)?;
    let data = rlp_left_pad::<N>(data)?;

    Ok(data)
}

#[inline]
pub fn rlp_decode_u256(buffer: &mut &[u8]) -> Result<U256> {
    let data: &[u8] = alloy_rlp::Header::decode_bytes(buffer, false)?;
    let data = rlp_left_pad::<32>(data)?;

    Ok(U256::from_be_bytes(data))
}

#[inline]
pub fn rlp_decode_opt_address(buffer: &mut &[u8]) -> Result<Option<Address>> {
    let data: &[u8] = alloy_rlp::Header::decode_bytes(buffer, false)?;

    if data.is_empty() {
        return Ok(None);
    }

    let data: [u8; 20] = data.try_into()?;
    Ok(Some(Address(data)))
}

#[inline]
pub fn rlp_decode_offsets(buffer: &mut &[u8], within: &[u8]) -> Result<Range<usize>> {
    let range = alloy_rlp::Header::decode_bytes(buffer, false)?.as_ptr_range();

    let start = unsafe { range.start.offset_from(within.as_ptr()) as usize };
    let end = unsafe { range.end.offset_from(within.as_ptr()) as usize };

    Ok(start..end)
}

#[inline]
pub fn rlp_list_header(list: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    alloy_rlp::Header {
        payload_length: list.len(),
        list: true,
    }
    .encode(&mut header);

    header
}

#[inline]
pub fn concat_signature(r: &[u8; 32], s: &[u8; 32]) -> [u8; 64] {
    const UNINIT: MaybeUninit<u8> = MaybeUninit::uninit();
    let mut signature = [UNINIT; 64];

    unsafe {
        let ptr = signature.as_mut_ptr().cast();

        std::ptr::copy_nonoverlapping(r.as_ptr(), ptr, 32);
        std::ptr::copy_nonoverlapping(s.as_ptr(), ptr.add(32), 32);

        std::mem::transmute(signature)
    }
}
