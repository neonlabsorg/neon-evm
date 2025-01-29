use ethnum::U256;

use crate::debug::log_data;

#[repr(C)]
#[derive(Clone)]
pub struct BlockParams {
    pub block_number: U256,
    pub block_timestamp: U256,
}

impl BlockParams {
    pub fn log_data(&self) {
        log_data(&[
            b"BLOCK",
            &self.block_number.to_le_bytes(),
            &self.block_timestamp.to_le_bytes(),
        ]);
    }
}
