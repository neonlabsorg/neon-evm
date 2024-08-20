use crate::types::vector::VectorSliceExt;
use crate::types::Vector;

#[must_use]
pub fn datacopy(input: &[u8]) -> Vector<u8> {
    debug_print!("datacopy");

    input.to_vector()
}
