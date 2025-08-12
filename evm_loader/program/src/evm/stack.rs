use std::{alloc::Layout, convert::TryInto, ptr::NonNull};

use allocator_api2::alloc::Allocator;
use ethnum::{I256, U256};

use crate::{error::Error, types::Address};

const ELEMENT_SIZE: usize = 32;
const STACK_SIZE: usize = ELEMENT_SIZE * 128;

#[repr(C)]
pub struct Stack<A: Allocator> {
    begin: NonNull<u8>,
    end: NonNull<u8>,
    top: NonNull<u8>,
    allocator: A,
}

impl<A: Allocator> Stack<A> {
    pub fn new_in(allocator: A) -> Self {
        let (begin, end) = unsafe {
            let layout = Layout::from_size_align_unchecked(STACK_SIZE, ELEMENT_SIZE);
            let Ok(begin) = allocator.allocate(layout) else {
                std::alloc::handle_alloc_error(layout);
            };

            let begin = begin.cast::<u8>();
            let end = begin.add(STACK_SIZE - ELEMENT_SIZE);

            (begin, end)
        };

        Self {
            begin,
            end,
            top: begin,
            allocator,
        }
    }

    pub fn reset(&mut self) {
        self.top = self.begin;
    }

    #[allow(unused)]
    pub fn to_vec(&self) -> Vec<[u8; 32]> {
        let slice = unsafe {
            let start = self.begin.cast::<[u8; 32]>();
            let end = self.top.cast::<[u8; 32]>();

            let len = end.offset_from(start).try_into().unwrap();
            std::slice::from_raw_parts(NonNull::as_ptr(start), len)
        };
        slice.to_vec()
    }

    #[inline(always)]
    unsafe fn read(&self) -> &[u8; 32] {
        self.top.cast::<[u8; 32]>().as_ref()
    }

    #[inline(always)]
    fn push(&mut self) -> Result<(), Error> {
        if self.top == self.end {
            return Err(Error::StackOverflow);
        }

        unsafe {
            self.top = self.top.add(32);
        }

        Ok(())
    }

    #[inline(always)]
    fn pop(&mut self) -> Result<(), Error> {
        if self.top == self.begin {
            return Err(Error::StackUnderflow);
        }

        unsafe {
            self.top = self.top.sub(32);
        }

        Ok(())
    }

    #[inline(always)]
    pub fn pop_u256(&mut self) -> Result<U256, Error> {
        self.pop()?;
        let a: [u8; 32] = unsafe { *self.read() };

        Ok(U256::from_be_bytes(a))
    }

    #[inline(always)]
    pub fn pop_i256(&mut self) -> Result<I256, Error> {
        self.pop()?;
        let a: [u8; 32] = unsafe { *self.read() };

        Ok(I256::from_be_bytes(a))
    }

    #[inline(always)]
    pub fn pop_array(&mut self) -> Result<&[u8; 32], Error> {
        self.pop()?;
        let a: &[u8; 32] = unsafe { self.read() };

        Ok(a)
    }

    #[inline(always)]
    pub fn pop_usize(&mut self) -> Result<usize, Error> {
        let value = self.pop_u256()?;
        let value = value.try_into()?;

        Ok(value)
    }

    #[inline(always)]
    pub fn pop_address(&mut self) -> Result<Address, Error> {
        const {
            assert!(std::mem::align_of::<Address>() == 1);
            assert!(std::mem::size_of::<Address>() == 20);
        };

        self.pop()?;

        let address = unsafe {
            let ptr = self.top.add(12); // discard 12 bytes
            ptr.cast::<Address>().read()
        };

        Ok(address)
    }

    #[inline(always)]
    pub fn discard(&mut self) -> Result<(), Error> {
        self.pop()
    }

    #[inline(always)]
    pub fn push_byte(&mut self, value: u8) -> Result<(), Error> {
        unsafe {
            self.top.write_bytes(0, 32);

            let ptr = self.top.add(31);
            ptr.write(value);
        }

        self.push()
    }

    #[inline(always)]
    pub fn push_zero(&mut self) -> Result<(), Error> {
        unsafe {
            self.top.write_bytes(0, 32);
        }

        self.push()
    }

    #[inline(always)]
    pub fn push_array(&mut self, value: &[u8; 32]) -> Result<(), Error> {
        unsafe {
            core::ptr::copy_nonoverlapping(value.as_ptr(), self.top.as_ptr(), 32);
        }

        self.push()
    }

    #[inline(always)]
    pub fn push_array_2_31<const N: usize>(&mut self, value: &[u8; N]) -> Result<(), Error> {
        // N >= 2 && N <= 31
        let zero_bytes: usize = 32 - N;

        unsafe {
            self.top.write_bytes(0, zero_bytes);

            let ptr = self.top.add(zero_bytes);
            core::ptr::copy_nonoverlapping(value.as_ptr(), NonNull::as_ptr(ptr), N);
        }

        self.push()
    }

    #[inline(always)]
    pub fn push_bool(&mut self, value: bool) -> Result<(), Error> {
        if value {
            self.push_byte(1_u8)
        } else {
            self.push_zero()
        }
    }

    #[inline(always)]
    pub fn push_address(&mut self, address: &Address) -> Result<(), Error> {
        let Address(value) = address;
        self.push_array_2_31::<20>(value)
    }

    #[inline(always)]
    pub fn push_usize(&mut self, value: usize) -> Result<(), Error> {
        let value: [u8; 8] = value.to_be_bytes();
        self.push_array_2_31::<8>(&value)
    }

    #[inline(always)]
    pub fn push_u256(&mut self, value: U256) -> Result<(), Error> {
        let value: [u8; 32] = value.to_be_bytes();
        self.push_array(&value)
    }

    #[inline(always)]
    pub fn push_i256(&mut self, value: I256) -> Result<(), Error> {
        let value: [u8; 32] = value.to_be_bytes();
        self.push_array(&value)
    }

    #[inline(always)]
    pub fn dup_1_16<const N: usize>(&mut self) -> Result<(), Error> {
        // N >= 1 && N <= 16

        let offset = unsafe { self.top.offset_from(self.begin) };
        if offset < (N * 32).try_into()? {
            return Err(Error::StackUnderflow);
        }

        unsafe {
            let source = self.top.sub(N * 32);
            let target = self.top;

            source.copy_to_nonoverlapping(target, 32);
        }

        self.push()
    }

    #[inline(always)]
    pub fn swap_1_16<const N: usize>(&mut self) -> Result<(), Error> {
        // N >= 1 && N <= 16

        let offset = unsafe { self.top.offset_from(self.begin) };
        if offset < ((N + 1) * 32).try_into()? {
            return Err(Error::StackUnderflow);
        }

        unsafe {
            let a = self.top.sub(32);
            let b = self.top.sub((N + 1) * 32);

            let mut c = [0_u8; 32];
            let c = NonNull::new_unchecked(c.as_mut_ptr());

            // compiler optimizes this into register operations
            a.copy_to_nonoverlapping(c, 32);
            b.copy_to_nonoverlapping(a, 32);
            c.copy_to_nonoverlapping(b, 32);
        }

        Ok(())
    }
}

impl<A: Allocator> Drop for Stack<A> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(STACK_SIZE, ELEMENT_SIZE);
            self.allocator.deallocate(self.begin, layout);
        }
    }
}
