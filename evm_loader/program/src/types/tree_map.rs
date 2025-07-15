use std::{
    fmt::{self, Debug, Display},
    hash::Hash,
    ops::Index,
};

use allocator_api2::alloc::{self, Allocator};

use super::Vector;

pub type IntoIter<K, V, A> = allocator_api2::vec::IntoIter<(K, V), A>;

#[derive(Clone)]
#[repr(C)]
pub struct TreeMap<K, V, A: Allocator> {
    entries: Vector<(K, V), A>,
}

impl<K, V> TreeMap<K, V, alloc::Global> {
    #[must_use]
    pub fn new() -> Self {
        TreeMap::new_in(alloc::Global)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        TreeMap {
            entries: Vector::with_capacity_in(capacity, alloc::Global),
        }
    }
}

impl<K, V, A: Allocator> TreeMap<K, V, A> {
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        TreeMap {
            entries: Vector::new_in(allocator),
        }
    }

    #[must_use]
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        TreeMap {
            entries: Vector::with_capacity_in(capacity, allocator),
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

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn drain(&mut self) -> impl Iterator<Item = (K, V)> + use<'_, K, V, A> {
        self.entries.drain(..)
    }
}

impl<K: Ord, V, A: Allocator> TreeMap<K, V, A> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries
            .binary_search_by_key(&key, |(k, _)| k)
            .map_or(Option::None, |idx| Option::Some(&self.entries[idx].1))
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries
            .binary_search_by_key(&key, |(k, _)| k)
            .map_or(Option::None, |idx| Option::Some(&mut self.entries[idx].1))
    }

    pub fn get_or_insert_with(&mut self, key: K, f: impl FnOnce(&K) -> V) -> &V {
        match self.entries.binary_search_by_key(&&key, |(k, _)| k) {
            Ok(idx) => &self.entries[idx].1,
            Err(idx) => {
                let value = f(&key);
                self.entries.insert(idx, (key, value));
                &self.entries[idx].1
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        match self.entries.binary_search_by_key(&&key, |(k, _)| k) {
            Ok(idx) => {
                self.entries[idx] = (key, value);
            }
            Err(idx) => {
                self.entries.insert(idx, (key, value));
            }
        }
    }

    pub fn insert_if_not_exists(&mut self, key: K, value: V) {
        if let Err(idx) = self.entries.binary_search_by_key(&&key, |(k, _)| k) {
            self.entries.insert(idx, (key, value));
        }
    }

    pub fn insert_with_if_not_exists<F>(&mut self, key: K, f: F)
    where
        F: FnOnce() -> V,
    {
        if let Err(idx) = self.entries.binary_search_by_key(&&key, |(k, _)| k) {
            let value = f();
            self.entries.insert(idx, (key, value));
        }
    }

    pub fn search(&self, key: &K) -> Result<usize, usize> {
        self.entries.binary_search_by_key(&key, |(k, _)| k)
    }

    /// # Safety
    /// It's a caller's responsibility to ensure that the `hint` is a valid index
    pub unsafe fn insert_with_hint(&mut self, key: K, value: V, hint: usize) {
        self.entries.insert(hint, (key, value));
    }

    pub fn update_or_insert<F>(&mut self, key: K, value: V, f: F)
    where
        F: FnOnce(&mut V),
        V: Clone,
    {
        match self.entries.binary_search_by_key(&&key, |(k, _)| k) {
            Ok(idx) => {
                let entry = &mut self.entries[idx];
                f(&mut entry.1);
            }
            Err(idx) => {
                self.entries.insert(idx, (key, value));
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self.entries.binary_search_by_key(&key, |(k, _)| k) {
            Ok(idx) => {
                let entry = self.entries.remove(idx);
                Some(entry.1)
            }
            Err(_) => None,
        }
    }

    pub fn remove_entry(&mut self, key: &K) -> Option<(K, V)> {
        match self.entries.binary_search_by_key(&key, |(k, _)| k) {
            Ok(idx) => Some(self.entries.remove(idx)),
            Err(_) => None,
        }
    }
}

impl<K: Ord + Copy, V> Default for TreeMap<K, V, alloc::Global> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Copy, V, A: Allocator> Index<K> for TreeMap<K, V, A> {
    type Output = V;

    fn index(&self, index: K) -> &Self::Output {
        self.get(&index).expect("no entry found for key")
    }
}

impl<K: Ord + Copy, V, A: Allocator> Index<&K> for TreeMap<K, V, A> {
    type Output = V;

    fn index(&self, index: &K) -> &Self::Output {
        self.get(index).expect("no entry found for key")
    }
}

impl<'a, K: 'a, V: 'a, A: Allocator> TreeMap<K, V, A> {
    pub fn iter(&'a self) -> std::slice::Iter<'a, (K, V)> {
        self.entries.iter()
    }

    pub fn iter_mut(&'a mut self) -> std::slice::IterMut<'a, (K, V)> {
        self.entries.iter_mut()
    }
}

impl<K, V, A: Allocator> IntoIterator for TreeMap<K, V, A> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a, K, V, A: Allocator> IntoIterator for &'a TreeMap<K, V, A> {
    type Item = &'a (K, V);
    type IntoIter = std::slice::Iter<'a, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V, A: Allocator> IntoIterator for &'a mut TreeMap<K, V, A> {
    type Item = &'a mut (K, V);
    type IntoIter = std::slice::IterMut<'a, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<K: Debug, V: Debug, A: Allocator> fmt::Debug for TreeMap<K, V, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = write!(f, "TreeMap {{");
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            res = res.and(write!(f, "{:?} -> {:?}, ", e.0, e.1));
        }
        res.and(write!(f, " }}"))
    }
}

impl<K: Display, V: Display, A: Allocator> fmt::Display for TreeMap<K, V, A> {
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

impl<K: Hash, V: Hash, A: Allocator> Hash for TreeMap<K, V, A> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entries.hash(state);
    }
}
