use std::{
    cmp::Ordering,
    fmt::{self, Debug, Display},
    hash::Hash,
    ops::Index,
    usize,
};

use super::Vector;
use crate::allocator::acc_allocator;

#[derive(Clone)]
#[repr(C)]
pub struct TreeMap<K, V> {
    entries: Vector<(K, V)>,
}

impl<K: Ord + Copy, V> TreeMap<K, V> {
    #[must_use]
    pub fn new() -> Self {
        TreeMap {
            entries: Vector::new_in(acc_allocator()),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        TreeMap {
            entries: Vector::with_capacity_in(capacity, acc_allocator()),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries
            .binary_search_by_key(key, |&(k, _)| k)
            .map_or(Option::None, |idx| Option::Some(&self.entries[idx].1))
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries
            .binary_search_by_key(key, |(k, _)| *k)
            .map_or(Option::None, |idx| Option::Some(&mut self.entries[idx].1))
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        V: Clone,
    {
        match self.entries.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(idx) => {
                // Clone is better in performance than potential vec realloc.
                let old = self.entries[idx].1.clone();
                self.entries[idx] = (key, value);
                Some(old)
            }
            Err(idx) => {
                self.entries.insert(idx, (key, value));
                None
            }
        }
    }

    pub fn update_or_insert<F, E>(&mut self, key: K, value: &V, f: F) -> Result<(), E>
    where
        F: FnOnce(V) -> Result<V, E>,
        V: Clone,
    {
        match self.entries.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(idx) => {
                let entry = &self.entries[idx];
                self.entries[idx] = (key, f(entry.1.clone())?);
            }
            Err(idx) => {
                self.entries.insert(idx, (key, value.clone()));
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self.entries.binary_search_by_key(key, |(k, _)| *k) {
            Ok(idx) => {
                let entry = self.entries.remove(idx);
                Some(entry.1)
            }
            Err(_) => None,
        }
    }

    pub fn remove_entry(&mut self, key: &K) -> Option<(K, V)> {
        match self.entries.binary_search_by_key(key, |(k, _)| *k) {
            Ok(idx) => Some(self.entries.remove(idx)),
            Err(_) => None,
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }
}

impl<K: Ord + Copy, V> Default for TreeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Copy, V> Index<K> for TreeMap<K, V> {
    type Output = V;

    fn index(&self, index: K) -> &Self::Output {
        self.get(&index).expect("no entry found for key")
    }
}

impl<K: Ord + Copy, V> FromIterator<(K, V)> for TreeMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut last_key: Option<K> = None;
        let mut entries = Vector::new_in(acc_allocator());

        for item in iter {
            let prev_key = last_key.replace(item.0);
            if let Some(ref prev) = prev_key {
                match Ord::cmp(prev, &item.0) {
                    Ordering::Less => (),
                    // Insert the last one from the consequtive list of equal keys.
                    Ordering::Equal => continue,
                    // Panic as we expect to have a valid iterator with non-decreasing keys.
                    Ordering::Greater => panic!("map keys should be non-decreasing"),
                }
            }
            entries.push(item);
        }

        TreeMap { entries }
    }
}

#[allow(clippy::iter_without_into_iter)]
impl<'a, K: 'a, V: 'a> TreeMap<K, V> {
    pub fn iter(&'a self) -> std::slice::Iter<'a, (K, V)> {
        self.entries.iter()
    }

    pub fn iter_mut(&'a mut self) -> std::slice::IterMut<'a, (K, V)> {
        self.entries.iter_mut()
    }
}

impl<K: Debug, V: Debug> fmt::Debug for TreeMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = write!(f, "TreeMap {{");
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            res = res.and(write!(f, "{:?} -> {:?}, ", e.0, e.1));
        }
        res.and(write!(f, " }}"))
    }
}

impl<K: Display, V: Display> fmt::Display for TreeMap<K, V> {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut res = write!(f, "TreeMap {{");
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            res = res.and(write!(f, "{} -> {}, ", e.0, e.1));
        }
        res.and(write!(f, " }}"))
    }
}

impl<K: Hash, V: Hash> Hash for TreeMap<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entries.hash(state);
    }
}
