#![allow(clippy::inline_always)]
use std::{
    cell::UnsafeCell,
    collections::{btree_map, BTreeMap},
    ptr::NonNull,
};

#[repr(transparent)]
pub struct BTreeMapCell<K, V>(UnsafeCell<BTreeMap<K, V>>);

impl<K, V> BTreeMapCell<K, V> {
    #[must_use]
    #[inline(always)]
    pub fn new() -> Self {
        Self(UnsafeCell::new(BTreeMap::new()))
    }

    #[must_use]
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn inner(&self) -> &mut BTreeMap<K, V> {
        NonNull::new_unchecked(self.0.get()).as_mut()
    }
}

impl<K: Ord + Copy, V: Copy> BTreeMapCell<K, V> {
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
        *map.entry(key).or_insert_with_key(|key| f(key))
    }

    #[inline(always)]
    pub fn insert(&self, key: K, value: V) {
        let map = unsafe { self.inner() };
        map.insert(key, value);
    }
}

impl<K, V> IntoIterator for BTreeMapCell<K, V> {
    type Item = (K, V);
    type IntoIter = btree_map::IntoIter<K, V>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_inner().into_iter()
    }
}

impl<K, V> Default for BTreeMapCell<K, V> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
