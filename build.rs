fn main() {
    let tmp_path = std::env::var("TMP_PATH").unwrap();
    println!("cargo:rustc-link-search=native={}/lib", tmp_path);
    println!("cargo:rustc-link-lib=static=omp");
}