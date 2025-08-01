use super::Vector;
use allocator_api2::alloc::{self, Allocator};

pub type IntoIter<V, A> = allocator_api2::vec::IntoIter<V, A>;
pub type Iter<'a, V> = core::slice::Iter<'a, V>;
pub type IterMut<'a, V> = core::slice::IterMut<'a, V>;

pub struct VacantEntry<'a, V, A: Allocator> {
    index: usize,
    v: &'a mut Vector<V, A>,
}

pub struct OccupiedEntry<'a, V, A: Allocator> {
    index: usize,
    v: &'a mut Vector<V, A>,
}

pub enum Entry<'a, V, A: Allocator> {
    Vacant(VacantEntry<'a, V, A>),
    Occupied(OccupiedEntry<'a, V, A>),
}

impl<'a, V, A: Allocator> Entry<'a, V, A> {
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
}

impl<'a, V, A: Allocator> VacantEntry<'a, V, A> {
    pub fn insert(self, value: V) -> OccupiedEntry<'a, V, A> {
        let VacantEntry { index, v } = self;

        v.insert(index, value);
        OccupiedEntry { index, v }
    }
}

impl<'a, V, A: Allocator> OccupiedEntry<'a, V, A> {
    #[must_use]
    pub fn get(&self) -> &V {
        let OccupiedEntry { index, v } = self;

        unsafe { v.get_unchecked(*index) }
    }

    pub fn get_mut(&mut self) -> &mut V {
        let OccupiedEntry { index, v } = self;

        unsafe { v.get_unchecked_mut(*index) }
    }

    #[must_use]
    pub fn into_mut(self) -> &'a mut V {
        let OccupiedEntry { index, v } = self;

        unsafe { v.get_unchecked_mut(index) }
    }

    #[must_use]
    pub fn remove(self) -> VacantEntry<'a, V, A> {
        let OccupiedEntry { index, v } = self;

        v.remove(index);
        VacantEntry { index, v }
    }

    pub fn replace(&mut self, value: V) -> V {
        std::mem::replace(self.get_mut(), value)
    }
}

pub struct VectorSet<V, A = alloc::Global>
where
    A: Allocator,
{
    v: Vector<V, A>,
}

impl<V> VectorSet<V, alloc::Global> {
    #[must_use]
    pub fn new() -> Self {
        VectorSet { v: Vector::new() }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        VectorSet {
            v: Vector::with_capacity(capacity),
        }
    }
}

impl<V, A> VectorSet<V, A>
where
    A: Allocator,
{
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        VectorSet {
            v: Vector::new_in(allocator),
        }
    }

    #[must_use]
    pub fn with_capacity_in(capacity: usize, allocator: A) -> Self {
        VectorSet {
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

    pub fn iter(&self) -> Iter<V> {
        self.v.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<V> {
        self.v.iter_mut()
    }

    #[must_use]
    pub fn drain(&mut self) -> allocator_api2::vec::Drain<V, A> {
        self.v.drain(..)
    }
}

impl<V: Ord, A: Allocator> VectorSet<V, A> {
    #[must_use]
    pub fn entry(&mut self, value: &V) -> Entry<V, A> {
        match self.v.binary_search(value) {
            Ok(index) => Entry::Occupied(OccupiedEntry {
                index,
                v: &mut self.v,
            }),
            Err(index) => Entry::Vacant(VacantEntry {
                index,
                v: &mut self.v,
            }),
        }
    }

    #[must_use]
    pub fn find_entry(&mut self, value: &V) -> Option<OccupiedEntry<V, A>> {
        let index = self.find_index(value)?;

        Some(OccupiedEntry {
            index,
            v: &mut self.v,
        })
    }

    #[must_use]
    #[inline(always)]
    fn find_index(&self, value: &V) -> Option<usize> {
        self.v.binary_search(value).ok()
    }

    pub fn insert(&mut self, value: V) {
        match self.entry(&value) {
            Entry::Occupied(mut entry) => {
                entry.replace(value);
            }
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
    }

    pub fn remove(&mut self, value: &V) {
        let Some(index) = self.find_index(value) else {
            return;
        };

        self.v.remove(index);
    }

    #[must_use]
    pub fn contains(&self, value: &V) -> bool {
        self.find_index(value).is_some()
    }
}

impl<V, A> IntoIterator for VectorSet<V, A>
where
    A: Allocator,
{
    type Item = V;
    type IntoIter = IntoIter<V, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.into_iter()
    }
}

impl<'a, V, A> IntoIterator for &'a VectorSet<V, A>
where
    A: Allocator,
{
    type Item = &'a V;
    type IntoIter = Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.iter()
    }
}

impl<'a, V, A> IntoIterator for &'a mut VectorSet<V, A>
where
    A: Allocator,
{
    type Item = &'a mut V;
    type IntoIter = IterMut<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.v.iter_mut()
    }
}
