pub mod ast;
pub mod bundle;
pub mod modules;
pub mod ts;
pub mod util;

#[cfg(feature = "ffi")]
pub mod ffi;

#[derive(Debug)]
#[allow(dead_code)] // used in cli
pub struct VersionInfo {
    build_date: &'static str,
    build_timestamp: &'static str,
    cargo_debug: &'static str,
    cargo_features: &'static str,
    cargo_opt_level: &'static str,
    cargo_target_triple: &'static str,
    cargo_dependencies: &'static str,
    rustc_channel: &'static str,
    rustc_commit_date: &'static str,
    rustc_commit_hash: &'static str,
    rustc_host_triple: &'static str,
    rustc_llvm_version: &'static str,
    rustc_semver: &'static str,
    git_branch: &'static str,
    git_commit_author_email: &'static str,
    git_commit_author_name: &'static str,
    git_commit_count: &'static str,
    git_commit_date: &'static str,
    git_commit_message: &'static str,
    git_commit_timestamp: &'static str,
    git_describe: &'static str,
}

pub const VERSION_INFO: VersionInfo = VersionInfo {
    build_date: env!("VERGEN_BUILD_DATE"),
    build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),
    cargo_debug: env!("VERGEN_CARGO_DEBUG"),
    cargo_features: env!("VERGEN_CARGO_FEATURES"),
    cargo_opt_level: env!("VERGEN_CARGO_OPT_LEVEL"),
    cargo_target_triple: env!("VERGEN_CARGO_TARGET_TRIPLE"),
    cargo_dependencies: env!("VERGEN_CARGO_DEPENDENCIES"),
    rustc_channel: env!("VERGEN_RUSTC_CHANNEL"),
    rustc_commit_date: env!("VERGEN_RUSTC_COMMIT_DATE"),
    rustc_commit_hash: env!("VERGEN_RUSTC_COMMIT_HASH"),
    rustc_host_triple: env!("VERGEN_RUSTC_HOST_TRIPLE"),
    rustc_llvm_version: env!("VERGEN_RUSTC_LLVM_VERSION"),
    rustc_semver: env!("VERGEN_RUSTC_SEMVER"),
    git_branch: env!("VERGEN_GIT_BRANCH"),
    git_commit_author_email: env!("VERGEN_GIT_COMMIT_AUTHOR_EMAIL"),
    git_commit_author_name: env!("VERGEN_GIT_COMMIT_AUTHOR_NAME"),
    git_commit_count: env!("VERGEN_GIT_COMMIT_COUNT"),
    git_commit_date: env!("VERGEN_GIT_COMMIT_DATE"),
    git_commit_message: env!("VERGEN_GIT_COMMIT_MESSAGE"),
    git_commit_timestamp: env!("VERGEN_GIT_COMMIT_TIMESTAMP"),
    git_describe: env!("VERGEN_GIT_DESCRIBE"),
};

pub fn print_version(verbose: bool) {
    let version = env!("CARGO_PKG_VERSION");
    if !verbose {
        println!("{}", version);
        return;
    }

    println!("Divvun Runtime v{}", version);
    println!("{:#?}", VERSION_INFO);
}

pub fn print_modules() {
    for module in modules::get_modules().iter() {
        println!("{}", module);
    }
}
