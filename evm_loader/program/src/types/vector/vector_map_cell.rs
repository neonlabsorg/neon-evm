use std::{cell::UnsafeCell, ptr::NonNull};

use super::vector_map::{Entry, VectorMap};

use allocator_api2::alloc::{self, Allocator};

#[repr(transparent)]
pub struct VectorMapCell<K, V, A: Allocator>(UnsafeCell<VectorMap<K, V, A>>);

impl<K, V> VectorMapCell<K, V, alloc::Global> {
    #[must_use]
    pub fn new() -> Self {
        Self::new_in(alloc::Global)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, alloc::Global)
    }
}

impl<K, V, A: Allocator> VectorMapCell<K, V, A> {
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        Self(UnsafeCell::new(VectorMap::new_in(allocator)))
    }

    #[must_use]
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        Self(UnsafeCell::new(VectorMap::with_capacity_in(
            capacity, allocator,
        )))
    }

    #[must_use]
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn inner(&self) -> &mut VectorMap<K, V, A> {
        NonNull::new_unchecked(self.0.get()).as_mut()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        let map = unsafe { self.inner() };
        map.is_empty()
    }
}

impl<K: Ord, V: Copy, A: Allocator> VectorMapCell<K, V, A> {
    #[must_use]
    pub fn get(&self, key: &K) -> Option<V> {
        let map = unsafe { self.inner() };
        map.get(key).copied()
    }

    #[must_use]
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce(&K) -> V,
    {
        let map = unsafe { self.inner() };
        *map.entry(key).or_insert_with_key(f)
    }

    pub fn update_or_insert<F>(&self, key: K, default: V, update: F)
    where
        F: FnOnce(&mut V),
    {
        let map = unsafe { self.inner() };
        match map.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(default);
            }
            Entry::Occupied(entry) => {
                update(entry.into_mut());
            }
        };
    }

    pub fn insert(&self, key: K, value: V) {
        let map = unsafe { self.inner() };
        map.insert(key, value);
    }
}

impl<K, V, A: Allocator> IntoIterator for VectorMapCell<K, V, A> {
    type Item = (K, V);
    type IntoIter = super::vector_map::IntoIter<K, V, A>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_inner().into_iter()
    }
}

impl<K, V> Default for VectorMapCell<K, V, alloc::Global> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
