#![allow(clippy::inline_always)]
use std::{cell::UnsafeCell, ptr::NonNull};

use allocator_api2::alloc::{self, Allocator};

use crate::types::TreeMap;

#[repr(transparent)]
pub struct TreeMapCell<K, V, A: Allocator>(UnsafeCell<TreeMap<K, V, A>>);

impl<K, V> TreeMapCell<K, V, alloc::Global> {
    #[must_use]
    #[inline(always)]
    pub fn new() -> Self {
        Self::new_in(alloc::Global)
    }

    #[must_use]
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, alloc::Global)
    }
}

impl<K, V, A: Allocator> TreeMapCell<K, V, A> {
    #[must_use]
    #[inline(always)]
    pub fn new_in(allocator: A) -> Self {
        Self(UnsafeCell::new(TreeMap::new_in(allocator)))
    }

    #[must_use]
    #[inline(always)]
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        Self(UnsafeCell::new(TreeMap::with_capacity_in(
            capacity, allocator,
        )))
    }

    #[must_use]
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn inner(&self) -> &mut TreeMap<K, V, A> {
        NonNull::new_unchecked(self.0.get()).as_mut()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        let map = unsafe { self.inner() };
        map.is_empty()
    }

    #[inline(always)]
    pub fn drain(&mut self) -> impl Iterator<Item = (K, V)> + use<'_, K, V, A> {
        self.0.get_mut().drain()
    }
}

impl<K: Ord + Copy, V: Copy, A: Allocator> TreeMapCell<K, V, A> {
    #[must_use]
    #[inline(always)]
    pub fn get(&self, key: &K) -> Option<V> {
        let map = unsafe { self.inner() };
        map.get(key).copied()
    }

    #[must_use]
    #[inline(always)]
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce(&K) -> V,
    {
        let map = unsafe { self.inner() };
        *map.get_or_insert_with(key, f)
    }

    #[inline(always)]
    pub fn update_or_insert<F>(&self, key: K, value: V, f: F)
    where
        F: FnOnce(&mut V),
    {
        let map = unsafe { self.inner() };
        map.update_or_insert(key, value, f);
    }

    #[inline(always)]
    pub fn insert(&self, key: K, value: V) {
        let map = unsafe { self.inner() };
        map.insert(key, value);
    }

    /// # Safety
    /// It's the caller's responsibility to ensure that the map is not modified while iterating.
    #[inline(always)]
    pub unsafe fn keys(&self) -> impl Iterator<Item = &K> + use<'_, K, V, A> {
        let map = unsafe { self.inner() };
        map.keys()
    }
}

impl<K, V, A: Allocator> IntoIterator for TreeMapCell<K, V, A> {
    type Item = (K, V);
    type IntoIter = super::tree_map::IntoIter<K, V, A>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_inner().into_iter()
    }
}

impl<K, V> Default for TreeMapCell<K, V, alloc::Global> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
