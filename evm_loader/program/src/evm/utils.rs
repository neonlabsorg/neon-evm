use std::slice::SliceIndex;

use allocator_api2::alloc::Allocator;
use allocator_api2::boxed::Box as Box2;

use crate::{
    evm::{tracing::EventListener, Machine},
    types::{vector::VectorSliceExt, Vector},
};

pub fn checked_next_multiple_of_32(n: usize) -> Option<usize> {
    Some(n.checked_add(31)? & !31)
}

type Parent<A, T> = Option<Box2<Machine<A, T>, A>>;

pub enum Buffer<A: Allocator> {
    Vec { v: Vector<u8, A> },
    ParentMemory { offset: usize, length: usize },
}

impl<A: Allocator + Copy> Buffer<A> {
    pub fn from_vec(v: Vector<u8, A>) -> Self {
        Buffer::Vec { v }
    }

    pub fn from_slice_in(v: &[u8], allocator: A) -> Self {
        Buffer::Vec {
            v: v.to_vector(allocator),
        }
    }

    pub fn from_memory(offset: usize, length: usize) -> Self {
        Buffer::ParentMemory { offset, length }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Buffer::Vec { v } => v.len(),
            Buffer::ParentMemory { length, .. } => *length,
        }
    }

    #[inline]
    pub fn as_slice<'a, T>(&'a self, parent: &'a Parent<A, T>) -> &'a [u8]
    where
        T: EventListener,
    {
        match self {
            Buffer::Vec { v } => v.as_slice(),
            Buffer::ParentMemory { offset, length } => {
                parent.as_ref().unwrap().memory.slice(*offset, *length)
            }
        }
    }

    #[inline]
    pub unsafe fn as_ptr<'a, T>(&'a self, parent: &'a Parent<A, T>) -> *const u8
    where
        T: EventListener,
    {
        self.as_slice(parent).as_ptr()
    }

    #[inline]
    pub fn get<'a, T, I>(&'a self, index: I, parent: &'a Parent<A, T>) -> Option<&'a I::Output>
    where
        T: EventListener,
        I: SliceIndex<[u8]>,
    {
        self.as_slice(parent).get(index)
    }

    #[inline]
    pub unsafe fn get_unchecked<'a, T, I>(
        &'a self,
        index: I,
        parent: &'a Parent<A, T>,
    ) -> &'a I::Output
    where
        T: EventListener,
        I: SliceIndex<[u8]>,
    {
        self.as_slice(parent).get_unchecked(index)
    }

    #[inline]
    pub fn get_u8<'a, T>(&'a self, index: usize, parent: &'a Parent<A, T>) -> u8
    where
        T: EventListener,
    {
        self.get(index, parent).copied().unwrap_or_default()
    }

    #[inline]
    #[allow(unused)]
    pub fn to_vec<T: EventListener>(&self, parent: &Parent<A, T>) -> Vec<u8> {
        self.as_slice(parent).to_vec()
    }
}
