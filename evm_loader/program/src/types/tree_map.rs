use allocator_api2::vec::Vec as CVec;

use std::{
    fmt::{self, Debug, Display},
    hash::Hash,
    iter::Zip,
    usize,
};

use crate::allocator::AccountAllocator;

pub struct BTreeMap<K, V> {
    keys: CVec<K, AccountAllocator>,
    values: CVec<V, AccountAllocator>,
}

impl<K: Ord, V> BTreeMap<K, V> {
    pub fn new_in(allocator: AccountAllocator) -> Self {
        BTreeMap {
            keys: CVec::new_in(allocator),
            values: CVec::new_in(allocator),
        }
    }

    pub fn with_capacity_in(capacity: usize, allocator: AccountAllocator) -> Self {
        BTreeMap {
            keys: CVec::with_capacity_in(capacity, allocator),
            values: CVec::with_capacity_in(capacity, allocator),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.keys.binary_search(&key) {
            Ok(idx) => Option::Some(&self.values[idx]),
            Err(_) => Option::None,
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self.keys.binary_search(&key) {
            Ok(_idx) => Option::Some(&mut self.values[_idx]),
            Err(_idx) => Option::None,
        }
    }

    pub fn insert(&mut self, key: K, value: &V) -> Option<V>
    where
        V: Clone,
    {
        match self.keys.binary_search(&key) {
            Ok(idx) => {
                // Clone is better in performance than potential vec realloc.
                let old = self.values[idx].clone();
                self.values.insert(idx, value.clone());
                Some(old)
            }
            Err(idx) => {
                self.keys.insert(idx, key);
                self.values.insert(idx, value.clone());
                None
            }
        }
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        match self.keys.binary_search(&key) {
            Ok(idx) => {
                self.keys.remove(idx);
                Some(self.values.remove(idx))
            }
            Err(_) => None,
        }
    }

    pub fn remove_entry(&mut self, key: K) -> Option<(K, V)> {
        match self.keys.binary_search(&key) {
            Ok(idx) => Some((self.keys.remove(idx), self.values.remove(idx))),
            Err(_) => None,
        }
    }
}

impl<'a, K: 'a, V: 'a> BTreeMap<K, V> {
    pub fn iter(&'a self) -> Zip<std::slice::Iter<'a, K>, std::slice::Iter<'a, V>> {
        std::iter::zip(self.keys.iter(), self.values.iter())
    }

    pub fn iter_mut(&'a mut self) -> Zip<std::slice::IterMut<'a, K>, std::slice::IterMut<'a, V>> {
        std::iter::zip(self.keys.iter_mut(), self.values.iter_mut())
    }
}

impl<K: Debug, V: Debug> fmt::Debug for BTreeMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = write!(f, "CBTreeMap {{");
        for i in 0..self.keys.len() {
            res = res.and(write!(f, "{:?} -> {:?}, ", self.keys[i], self.values[i]));
        }
        res.and(write!(f, " }}"))
    }
}

impl<K: Display, V: Display> fmt::Display for BTreeMap<K, V> {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut res = write!(f, "CBTreeMap {{");
        for i in 0..self.keys.len() {
            res = res.and(write!(f, "{} -> {}, ", self.keys[i], self.values[i]));
        }
        res.and(write!(f, " }}"))
    }
}

impl<K: Hash, V: Hash> Hash for BTreeMap<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.keys.hash(state);
        self.values.hash(state);
    }
}
