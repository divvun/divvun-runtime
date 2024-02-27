#[cfg(windows)]
fn main() {
    vcpkg::Config::new().find_package("icu").unwrap();
}

#[cfg(not(windows))]
fn main() {}
