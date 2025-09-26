#ifndef DIVVUN_RUNTIME_FFI_H
#define DIVVUN_RUNTIME_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifndef __APPLE__
#define _Nonnull
#define _Nullable
#endif

typedef uintptr_t rust_usize_t;

// Rust slice type for passing strings
typedef struct rust_slice_s {
    void *_Nullable data;
    rust_usize_t len;
} rust_slice_t;

// Error callback function type
typedef void (*error_callback_t)(void *_Nullable error_ptr, rust_usize_t error_len);

// Opaque handle types
typedef void* bundle_handle_t;
typedef void* pipeline_handle_t;

// Bundle functions
bundle_handle_t _Nullable DRT_Bundle_fromBundle(rust_slice_t bundle_path, error_callback_t _Nonnull error_callback);
bundle_handle_t _Nullable DRT_Bundle_fromPath(rust_slice_t path, error_callback_t _Nonnull error_callback);
void DRT_Bundle_drop(bundle_handle_t _Nonnull bundle);

// Pipeline functions
pipeline_handle_t _Nullable DRT_Bundle_create(bundle_handle_t _Nonnull bundle, rust_slice_t config, error_callback_t _Nonnull error_callback);
void DRT_PipelineHandle_drop(pipeline_handle_t _Nonnull handle);

// Forward function returns a rust_slice_t with the output data
rust_slice_t DRT_PipelineHandle_forward(pipeline_handle_t _Nonnull handle, rust_slice_t input, error_callback_t _Nonnull error_callback);

// Memory management for Rust-allocated vectors
void DRT_Vec_drop(rust_slice_t vec);

#ifdef __cplusplus
}
#endif

#endif // DIVVUN_RUNTIME_FFI_H