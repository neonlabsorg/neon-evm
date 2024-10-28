use ethnum::U256;

#[repr(C)]
#[derive(Clone)]
pub struct BlockParams {
    pub block_number: U256,
    pub block_timestamp: U256,
}
