use ethnum::U256;
pub use log_data_derive::LogData;
pub struct Address(pub [u8; 20]);

pub trait LogData {
    fn log_data(&self);
}

pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

impl ToBytes for usize {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl ToBytes for u32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl ToBytes for u64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl ToBytes for i32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl ToBytes for String {
    fn to_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl<const N: usize> ToBytes for [u8; N] {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl ToBytes for U256 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

// impl ToBytes for Address {
//     fn to_bytes(&self) -> Vec<u8> {
//         &self.as_bytes()
//     }
// }
//
// impl ToBytes for &Address {
//     fn to_bytes(self) -> Vec<u8> {
//         self.to_vec()
//     }
// }
