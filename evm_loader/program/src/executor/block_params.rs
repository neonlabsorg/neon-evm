use ethnum::U256;

use crate::debug::log_data;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlockParams {
    pub number: U256,
    pub timestamp: U256,
}

impl BlockParams {
    pub fn log_data(&self) {
        log_data(&[
            b"BLOCK",
            &self.number.to_le_bytes(),
            &self.timestamp.to_le_bytes(),
        ]);
    }
}
