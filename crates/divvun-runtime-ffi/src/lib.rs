//! Cdylib wrapper that re-exports divvun-runtime's `DRT_*` FFI surface so
//! that consumers needing a dynamically-loaded library (LoadLibrary / dlopen)
//! get a `libdivvun_runtime.{so,dylib}` / `divvun_runtime.dll` with the
//! canonical symbols.
//!
//! The parent `divvun-runtime` crate is built as `["lib", "staticlib"]`; on
//! Windows MSVC, also asking for `cdylib` from the same crate collides on
//! `divvun_runtime.lib` (cargo issue #8718). Splitting the cdylib into this
//! sibling crate sidesteps the collision.

#![allow(non_snake_case)]

pub use divvun_runtime::ffi::*;
