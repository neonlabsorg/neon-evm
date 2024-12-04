use neon_lib_interface::NeonEVMLibRef;
use std::collections::HashMap;

pub struct Context {
    pub libraries: HashMap<String, NeonEVMLibRef>,
}
