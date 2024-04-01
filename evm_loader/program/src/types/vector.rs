use crate::allocator::{acc_allocator, StateAccountAllocator};

pub type Vector<T> = allocator_api2::vec::Vec<T, StateAccountAllocator>;

#[macro_export]
macro_rules! vector {
    () => (
        allocator_api2::vec::Vec::with_capacity_in(1, $crate::allocator::acc_allocator())
    );
    ($elem:expr; $n:expr) => (
        allocator_api2::vec::from_elem_in($elem, $n, $crate::allocator::acc_allocator())
    );
    ($($x:expr),+ $(,)?) => (
        allocator_api2::boxed::Box::<[_], $crate::allocator::StateAccountAllocator>::into_vec(
            allocator_api2::boxed::Box::slice(
                allocator_api2::boxed::Box::new_in([$($x),+], $crate::allocator::acc_allocator())
            )
        )
    );
}

#[must_use]
pub fn into_vector<T>(v: Vec<T>) -> Vector<T> {
    let mut ret = Vector::with_capacity_in(v.len(), acc_allocator());
    for item in v {
        ret.push(item);
    }
    ret
}

#[must_use]
pub fn vect<T>() -> Vector<T> {
    Vector::with_capacity_in(1, acc_allocator())
}