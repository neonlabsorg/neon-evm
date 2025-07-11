use std::alloc::Layout;
use std::ops::Range;
use std::ptr::NonNull;

use allocator_api2::alloc::Allocator;
use solana_program::program_memory::{sol_memcpy, sol_memset};

use crate::error::Error;

use super::utils::checked_next_multiple_of_32;

const MAX_MEMORY_SIZE: usize = 64 * 1024;
const MEMORY_CAPACITY: usize = 1024;
const MEMORY_ALIGN: usize = 1;

static_assertions::const_assert!(MEMORY_ALIGN.is_power_of_two());

#[repr(C)]
pub struct Memory<A: Allocator> {
    data: NonNull<[u8]>,
    capacity: usize,
    size: usize,
    allocator: A,
}

impl<A: Allocator> Memory<A> {
    pub fn new_in(allocator: A) -> Self {
        Self::with_capacity_in(MEMORY_CAPACITY, allocator)
    }

    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        unsafe {
            let layout = Layout::from_size_align_unchecked(capacity, MEMORY_ALIGN);
            let Ok(data) = allocator.allocate_zeroed(layout) else {
                std::alloc::handle_alloc_error(layout);
            };

            Self {
                data,
                capacity,
                size: 0,
                allocator,
            }
        }
    }

    pub fn reset(&mut self) {
        let slice = self.slice_mut(0, self.size);
        sol_memset(slice, 0, slice.len());

        self.size = 0;
    }

    #[allow(unused)]
    pub fn to_vec(&self) -> Vec<u8> {
        let slice: &[u8] = unsafe { &*self.data.as_ptr() };
        slice.to_vec()
    }

    #[inline]
    fn realloc(&mut self, offset: usize, length: usize) -> Result<(), Error> {
        let required_size = offset
            .checked_add(length)
            .ok_or(Error::MemoryAccessOutOfLimits(offset, length))?;

        let new_size = checked_next_multiple_of_32(required_size)
            .ok_or(Error::MemoryAccessOutOfLimits(offset, length))?;

        if new_size > self.size {
            self.size = new_size;
        }

        if new_size <= self.capacity {
            return Ok(());
        }

        let new_capacity = new_size
            .checked_next_power_of_two()
            .ok_or(Error::MemoryAccessOutOfLimits(offset, length))?;
        if new_capacity > MAX_MEMORY_SIZE {
            return Err(Error::MemoryAccessOutOfLimits(offset, length));
        }

        unsafe {
            let old_layout = Layout::from_size_align_unchecked(self.capacity, MEMORY_ALIGN);
            let new_layout = Layout::from_size_align_unchecked(new_capacity, MEMORY_ALIGN);
            let old_data: NonNull<u8> = self.data.cast();

            let Ok(new_data) = self.allocator.grow(old_data, old_layout, new_layout) else {
                std::alloc::handle_alloc_error(new_layout);
            };

            let slice: &mut [u8] = &mut *new_data.as_ptr();
            sol_memset(&mut slice[self.capacity..], 0, new_capacity - self.capacity);

            self.data = new_data;
            self.capacity = new_capacity;
        }

        Ok(())
    }

    #[inline]
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    #[must_use]
    pub fn slice(&self, offset: usize, length: usize) -> &[u8] {
        let slice = unsafe { self.data.as_ref() };
        &slice[offset..offset + length]
    }

    #[inline]
    #[must_use]
    pub fn slice_mut(&mut self, offset: usize, length: usize) -> &mut [u8] {
        let slice = unsafe { self.data.as_mut() };
        &mut slice[offset..offset + length]
    }

    pub fn read(&mut self, offset: usize, length: usize) -> Result<&[u8], Error> {
        if length == 0_usize {
            return Ok(&[]);
        }

        self.realloc(offset, length)?;

        Ok(self.slice(offset, length))
    }

    pub fn read_32(&mut self, offset: usize) -> Result<&[u8; 32], Error> {
        self.realloc(offset, 32)?;

        let slice = self.slice(offset, 32);
        let array = slice.try_into()?;

        Ok(array)
    }

    pub fn write_32(&mut self, offset: usize, value: &[u8; 32]) -> Result<(), Error> {
        self.realloc(offset, 32)?;

        let slice = self.slice_mut(offset, 32);
        slice.copy_from_slice(value);

        Ok(())
    }

    pub fn write_byte(&mut self, offset: usize, value: u8) -> Result<(), Error> {
        self.realloc(offset, 1)?;

        let slice = self.slice_mut(offset, 1);
        slice[0] = value;

        Ok(())
    }

    pub fn write(&mut self, offset: usize, source: &[u8]) -> Result<(), Error> {
        if source.is_empty() {
            return Ok(());
        }

        self.realloc(offset, source.len())?;

        let data = self.slice_mut(offset, source.len());
        data.copy_from_slice(source);

        Ok(())
    }

    pub fn write_buffer(
        &mut self,
        offset: usize,
        length: usize,
        source: &[u8],
        source_offset: usize,
    ) -> Result<(), Error> {
        if length == 0_usize {
            return Ok(());
        }

        self.realloc(offset, length)?;
        let data = self.slice_mut(offset, length);

        match source_offset {
            source_offset if source_offset >= source.len() => {
                sol_memset(data, 0, length);
            }
            source_offset if (source_offset + length) > source.len() => {
                let source = &source[source_offset..];

                data[..source.len()].copy_from_slice(source);
                data[source.len()..].fill(0_u8);
            }
            source_offset => {
                let source = &source[source_offset..source_offset + length];
                sol_memcpy(data, source, length);
            }
        }

        Ok(())
    }

    #[inline]
    pub fn write_range(&mut self, range: &Range<usize>, source: &[u8]) -> Result<(), Error> {
        self.write_buffer(range.start, range.len(), source, 0)
    }

    pub fn copy_within(
        &mut self,
        source: usize,
        destination: usize,
        length: usize,
    ) -> Result<(), Error> {
        if length == 0_usize {
            return Ok(());
        }

        // If length > 0 and (src + length or dst + length) is beyond the current memory length, the memory is extended
        self.realloc(std::cmp::max(source, destination), length)?;

        // SAFETY: self.realloc ensures that the memory is large enough to perform the memmove without out of bounds access
        let slice = unsafe { self.data.as_mut() };
        slice.copy_within(source..source + length, destination);

        Ok(())
    }
}

impl<A: Allocator> Drop for Memory<A> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.capacity, MEMORY_ALIGN);
            self.allocator.deallocate(self.data.cast(), layout);
        }
    }
}
