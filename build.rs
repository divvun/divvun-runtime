fn main() {
    // let tmp_path = std::env::var("TMP_PATH").unwrap();
    // println!("cargo:rustc-link-search=native={}/lib", tmp_path);
    // println!("cargo:rustc-link-lib=static=omp");
    if let Ok(path) = std::env::var("LIBTORCH") {
        println!(
            "{:?}",
            std::fs::read_dir(&std::path::Path::new(&path).join("lib"))
                .unwrap()
                .filter_map(Result::ok)
                .map(|x| x.path())
                .collect::<Vec<_>>()
        );
        println!("cargo:rustc-link-search=native={}/lib", path);
    }

    println!("cargo:rustc-link-lib=c10");
    println!("cargo:rustc-link-lib=torch");
    println!("cargo:rustc-link-lib=torch_cpu");
}
