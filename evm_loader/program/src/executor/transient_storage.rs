use allocator_api2::alloc::Allocator;
use ethnum::U256;

use crate::types::{Address, Vector};

struct Item {
    address: Address,
    index: U256,
    value: [u8; 32],
}

pub struct TransientStorage<A: Allocator> {
    storage: Vector<Item, A>,
    stack: Vector<usize, A>,
}

impl<A: Allocator + Copy> TransientStorage<A> {
    pub fn new_in(allocator: A) -> Self {
        Self {
            storage: Vector::new_in(allocator),
            stack: Vector::with_capacity_in(8, allocator),
        }
    }
}

impl<A: Allocator> TransientStorage<A> {
    pub fn read(&self, address: Address, index: U256) -> [u8; 32] {
        for item in self.storage.iter().rev() {
            if (item.address == address) && (item.index == index) {
                return item.value;
            }
        }

        // If not found, return the default value (zeroed out)
        [0; 32]
    }

    pub fn write(&mut self, address: Address, index: U256, value: [u8; 32]) {
        self.storage.push(Item {
            address,
            index,
            value,
        });
    }

    pub fn snapshot(&mut self) {
        self.stack.push(self.storage.len());
    }

    pub fn revert(&mut self) {
        let storage_len = self
            .stack
            .pop()
            .expect("Fatal Error: Inconsistent EVM Call Stack");

        self.storage.truncate(storage_len);
    }

    pub fn commit(&mut self) {
        self.stack
            .pop()
            .expect("Fatal Error: Inconsistent EVM Call Stack");
    }
}
