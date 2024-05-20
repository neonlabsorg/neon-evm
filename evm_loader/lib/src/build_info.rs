use crate::build_info_common::SlimBuildInfo;

build_info::build_info!(fn build_info);

#[must_use]
pub fn get_build_info() -> SlimBuildInfo {
    build_info().into()
}
