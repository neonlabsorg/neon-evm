use build_info_build::{GitInfo, VersionControl};

fn main() {
    let info = build_info_build::build_script().build();
    if let Some(VersionControl::Git(GitInfo { ref commit_id, .. })) = info.version_control {
        println!("cargo:rustc-env=STATE_SIGNATURE={commit_id}");
    }
}
