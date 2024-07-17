use std::cmp::{max, min};
use std::ptr::read_unaligned;
use std::slice;

pub trait ReconstructRaw {
    /// # Safety
    /// Function to reconstruct an object from the memory. Should be used by macro and not implemented manually.
    /// Also, it must not be used in the EVM Program, only in the Core API.
    unsafe fn build(ptr: *const Self, offset: isize) -> Self;
}

/// Reading the raw memory bits to reconstuct the Vec<T> type.
/// # Safety
/// Low level reads in the memory with offsets to reconstruct the vector.
#[must_use]
pub unsafe fn read_vec<T: Default + Copy>(vec_start_ptr: *const usize, offset: isize) -> Vec<T> {
    // 1. The Vector's memory layout consists of three usizes: ptr to the buffer, capacity and length.
    // 2. There's no alignment between the fields, the Vector occupies exactly the 3*sizeof<usize> bytes.
    // 3. The order of those fields in the memory is unspecified (no repr is set on the vec struct).
    // => The len is the smallest of those three usizes, because it can't realistically be more than the buffer
    // ptr value and it's no more than capacity.
    // => The buffer ptr is the biggest among them.
    let vec_parts = (
        read_unaligned(vec_start_ptr),
        read_unaligned(vec_start_ptr.add(1)),
        read_unaligned(vec_start_ptr.add(2)),
    );
    let vec_len = min(min(vec_parts.0, vec_parts.1), vec_parts.2);
    let vec_buf_ptr_unadjusted = max(max(vec_parts.0, vec_parts.1), vec_parts.2) as *const u8;
    // Offset the buffer pointer from the state account allocator memory space into the current allocator.
    let vec_buf_ptr_adjusted = vec_buf_ptr_unadjusted.offset(offset).cast::<T>().cast_mut();

    // Allocate a new vec and with the exact number of elements and copy the memory.
    let mut res_vec = vec![T::default(); vec_len];
    res_vec.copy_from_slice(slice::from_raw_parts(vec_buf_ptr_adjusted, vec_len));
    res_vec
}
