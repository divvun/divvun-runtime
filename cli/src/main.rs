#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    divvun_runtime_cli::run_cli().await
}

#[cfg(feature = "ffi")]
pub use divvun_runtime::ffi::*;

#[cfg(feature = "ffi")]
const _: () = {
    std::hint::black_box(DRT_Bundle_fromBundle);
};

#[no_mangle]
#[link_section = "__TEXT,__text"]
pub extern "C" fn DRT_my_exported_symbol() {
    println!("Hello from exported symbol!");
}

const _: () = {
    std::hint::black_box(DRT_my_exported_symbol);
};
