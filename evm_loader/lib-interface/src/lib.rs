#![allow(non_camel_case_types)]

pub mod types;

use crate::types::RNeonEVMLibResult;
use std::{collections::HashMap, path::Path};
use thiserror::Error;

use abi_stable::{
    library::{LibraryError, RootModule},
    package_version_strings,
    std_types::{RStr, RString},
    StableAbi,
};

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = NeonEVMLib_Ref)))]
#[sabi(missing_field(panic))]
pub struct NeonEVMLib {
    pub hash: extern "C" fn() -> RString,
    pub get_version: extern "C" fn() -> RString,
    pub get_build_info: extern "C" fn() -> RString,

    pub invoke: for<'a> extern "C" fn(RStr<'a>, RStr<'a>) -> RNeonEVMLibResult<'a>,
}

impl RootModule for NeonEVMLib_Ref {
    abi_stable::declare_root_module_statics! {NeonEVMLib_Ref}

    const BASE_NAME: &'static str = "neon-lib-interface";
    const NAME: &'static str = "neon-lib-interface";
    const VERSION_STRINGS: abi_stable::sabi_types::VersionStrings = package_version_strings!();
}

#[derive(Error, Debug)]
pub enum NeonEVMLibLoadError {
    #[error("abi_stable library error")]
    LibraryError(#[from] LibraryError),
    #[error("IO error")]
    IoError(#[from] std::io::Error),
}

pub fn load_libraries<P>(
    directory: P,
) -> Result<HashMap<String, NeonEVMLib_Ref>, NeonEVMLibLoadError>
where
    P: AsRef<Path>,
{
    let paths = std::fs::read_dir(directory)?;
    let mut result = HashMap::new();
    for path in paths {
        let lib = NeonEVMLib_Ref::load_from_file(&path?.path())?;
        let hash = lib.hash()();

        result.insert(hash.into_string(), lib);
    }
    Ok(result)
}
