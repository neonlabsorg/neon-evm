use std::ops::Index;

use super::Vector;
use allocator_api2::alloc::{self, Allocator};

pub type IntoIter<K, V, A> = allocator_api2::vec::IntoIter<(K, V), A>;
pub type Iter<'a, K, V> = core::slice::Iter<'a, (K, V)>;
pub type IterMut<'a, K, V> = core::slice::IterMut<'a, (K, V)>;

pub struct VacantEntry<'a, K, V, A: Allocator> {
    key: K,
    index: usize,
    v: &'a mut Vector<(K, V), A>,
}

pub struct OccupiedEntry<'a, K, V, A: Allocator> {
    index: usize,
    v: &'a mut Vector<(K, V), A>,
}

pub enum Entry<'a, K, V, A: Allocator> {
    Vacant(VacantEntry<'a, K, V, A>),
    Occupied(OccupiedEntry<'a, K, V, A>),
}

impl<'a, K, V, A: Allocator> Entry<'a, K, V, A> {
    pub fn or_insert(self, value: V) -> &'a mut V {
        match self {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(vacant_entry) => vacant_entry.insert(value),
        }
        .into_mut()
    }

    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        match self {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(vacant_entry) => vacant_entry.insert(f()),
        }
        .into_mut()
    }

    pub fn or_insert_with_key(self, f: impl FnOnce(&K) -> V) -> &'a mut V {
        match self {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(entry) => {
                let value = f(&entry.key);
                entry.insert(value)
            }
        }
        .into_mut()
    }
}

impl<'a, K, V, A: Allocator> VacantEntry<'a, K, V, A> {
    pub fn insert(self, value: V) -> OccupiedEntry<'a, K, V, A> {
        let VacantEntry { key, index, v } = self;

        v.insert(index, (key, value));
        OccupiedEntry { index, v }
    }
}

impl<'a, K, V, A: Allocator> OccupiedEntry<'a, K, V, A> {
    #[must_use]
    pub fn get(&self) -> &V {
        let OccupiedEntry { index, v } = self;

        let kv = unsafe { v.get_unchecked(*index) };
        &kv.1
    }

    pub fn get_mut(&mut self) -> &mut V {
        let OccupiedEntry { index, v } = self;

        let kv = unsafe { v.get_unchecked_mut(*index) };
        &mut kv.1
    }

    #[must_use]
    pub fn into_mut(self) -> &'a mut V {
        let OccupiedEntry { index, v } = self;

        let kv = unsafe { v.get_unchecked_mut(index) };
        &mut kv.1
    }

    #[must_use]
    pub fn remove(self) -> VacantEntry<'a, K, V, A> {
        let OccupiedEntry { index, v } = self;

        let kv = v.remove(index);
        VacantEntry {
            key: kv.0,
            index,
            v,
        }
    }

    pub fn replace(&mut self, value: V) -> V {
        std::mem::replace(self.get_mut(), value)
    }
}

pub struct VectorMap<K, V, A = alloc::Global>
where
    A: Allocator,
{
    v: Vector<(K, V), A>,
}

impl<K, V> VectorMap<K, V, alloc::Global> {
    #[must_use]
    pub fn new() -> Self {
        VectorMap { v: Vector::new() }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        VectorMap {
            v: Vector::with_capacity(capacity),
        }
    }
}

impl<K, V, A> VectorMap<K, V, A>
where
    A: Allocator,
{
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        VectorMap {
            v: Vector::new_in(allocator),
        }
    }

    #[must_use]
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        VectorMap {
            v: Vector::with_capacity_in(capacity, allocator),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.v.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }

    pub fn clear(&mut self) {
        self.v.clear();
    }

    pub fn iter(&self) -> Iter<K, V> {
        self.v.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<K, V> {
        self.v.iter_mut()
    }
}

impl<K: Ord, V, A: Allocator> VectorMap<K, V, A> {
    #[must_use]
    pub fn entry(&mut self, key: K) -> Entry<K, V, A> {
        match self.v.binary_search_by_key(&&key, |(k, _)| k) {
            Ok(index) => Entry::Occupied(OccupiedEntry {
                index,
                v: &mut self.v,
            }),
            Err(index) => Entry::Vacant(VacantEntry {
                key,
                index,
                v: &mut self.v,
            }),
        }
    }

    #[must_use]
    pub fn find_entry(&mut self, key: &K) -> Option<OccupiedEntry<K, V, A>> {
        let index = self.find_index(key)?;

        Some(OccupiedEntry {
            index,
            v: &mut self.v,
        })
    }

    #[must_use]
    #[inline(always)]
    fn find_index(&self, key: &K) -> Option<usize> {
        self.v.binary_search_by_key(&key, |(k, _)| k).ok()
    }

    pub fn insert(&mut self, key: K, value: V) {
        match self.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.replace(value);
            }
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
    }

    pub fn remove(&mut self, key: &K) {
        let Some(index) = self.find_index(key) else {
            return;
        };

        self.v.remove(index);
    }

    #[must_use]
    pub fn into_single_value(mut self, key: &K) -> Option<V> {
        let index = self.find_index(key)?;

        let kv = self.v.swap_remove(index);
        Some(kv.1)
    }

    #[must_use]
    pub fn get(&self, key: &K) -> Option<&V> {
        let index = self.find_index(key)?;

        let kv = unsafe { self.v.get_unchecked(index) };
        Some(&kv.1)
    }

    #[must_use]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = self.find_index(key)?;

        let kv = unsafe { self.v.get_unchecked_mut(index) };
        Some(&mut kv.1)
    }

    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.find_index(key).is_some()
    }
}

impl<K, V, A> IntoIterator for VectorMap<K, V, A>
where
    A: Allocator,
{
    type Item = (K, V);
    type IntoIter = IntoIter<K, V, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.into_iter()
    }
}

impl<'a, K, V, A> IntoIterator for &'a VectorMap<K, V, A>
where
    A: Allocator,
{
    type Item = &'a (K, V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.iter()
    }
}

impl<'a, K, V, A> IntoIterator for &'a mut VectorMap<K, V, A>
where
    A: Allocator,
{
    type Item = &'a mut (K, V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.iter_mut()
    }
}

impl<K, V> FromIterator<(K, V)> for VectorMap<K, V, alloc::Global>
where
    K: Ord,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut v = Vector::from_iter(iter);
        v.sort_unstable_by(|(k1, _), (k2, _)| k1.cmp(k2));

        Self { v }
    }
}

impl<A, K, V> Index<&K> for VectorMap<K, V, A>
where
    K: Ord,
    A: Allocator,
{
    type Output = V;

    #[track_caller]
    fn index(&self, key: &K) -> &Self::Output {
        self.get(key).unwrap()
    }
}
