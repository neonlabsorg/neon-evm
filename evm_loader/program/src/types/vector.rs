use allocator_api2::alloc::Allocator;
use allocator_api2::vec::Vec;
use allocator_api2::SliceExt;

pub type Vector<T, A> = Vec<T, A>;

pub trait VectorVecExt<T, A: Allocator> {
    fn into_vector(self, allocator: A) -> Vec<T, A>
    where
        T: Copy + Default;
}

pub trait VectorSliceExt<T, A: Allocator> {
    fn to_vector(&self, allocator: A) -> Vec<T, A>
    where
        T: Copy + Default;
}

pub trait VectorVecSlowExt<T, A: Allocator> {
    fn elementwise_copy_into_vector(self, allocator: A) -> Vec<T, A>
    where
        T: Clone;
}

pub trait VectorSliceSlowExt<T, A: Allocator> {
    fn elementwise_copy_to_vector(&self, allocator: A) -> Vec<T, A>
    where
        T: Clone;
}

impl<T: Copy, A: Allocator> VectorVecExt<T, A> for std::vec::Vec<T> {
    fn into_vector(self, allocator: A) -> Vec<T, A> {
        let mut ret = Vec::with_capacity_in(self.len(), allocator);
        // SAFETY:
        // allocated above with the capacity of `self.len()`, and initialize to `self.len()` in
        // ptr::copy_to_non_overlapping below.
        unsafe {
            self.as_ptr()
                .copy_to_nonoverlapping(ret.as_mut_ptr(), self.len());
            ret.set_len(self.len());
        }
        ret
    }
}

impl<T: Copy, A: Allocator> VectorSliceExt<T, A> for [T] {
    fn to_vector(&self, allocator: A) -> Vec<T, A> {
        let mut ret = Vec::with_capacity_in(self.len(), allocator);
        // SAFETY:
        // allocated above with the capacity of `self.len()`, and initialize to `self.len()` in
        // ptr::copy_to_non_overlapping below.
        unsafe {
            self.as_ptr()
                .copy_to_nonoverlapping(ret.as_mut_ptr(), self.len());
            ret.set_len(self.len());
        }
        ret
    }
}

impl<T, A: Allocator> VectorSliceSlowExt<T, A> for [T] {
    fn elementwise_copy_to_vector(&self, allocator: A) -> Vec<T, A>
    where
        T: Clone,
    {
        SliceExt::to_vec_in(self, allocator)
    }
}

impl<T, A: Allocator> VectorVecSlowExt<T, A> for std::vec::Vec<T> {
    fn elementwise_copy_into_vector(self, allocator: A) -> Vec<T, A> {
        let mut ret = Vec::with_capacity_in(self.len(), allocator);
        for item in self {
            ret.push(item);
        }
        ret
    }
}

pub fn seeds2_to_vector<A: Allocator + Copy>(seeds: &[&[u8]], allocator: A) -> Vec<Vec<u8, A>, A> {
    let mut ret = Vec::with_capacity_in(seeds.len(), allocator);
    for seed in seeds {
        ret.push(seed.to_vector(allocator));
    }
    ret
}

pub fn seeds3_to_vector<A: Allocator + Copy>(
    program_seeds: &[&[&[u8]]],
    allocator: A,
) -> Vec<Vec<Vec<u8, A>, A>, A> {
    let mut ret = Vec::with_capacity_in(program_seeds.len(), allocator);
    for account_seeds in program_seeds {
        let mut inner = Vec::with_capacity_in(account_seeds.len(), allocator);
        for seed in *account_seeds {
            inner.push(seed.to_vector(allocator));
        }

        ret.push(inner);
    }
    ret
}
