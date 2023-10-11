use neon_lib_interface::NeonEVMLib_Ref;
use std::collections::HashMap;

pub struct Context {
    pub libraries: HashMap<String, NeonEVMLib_Ref>,
}
