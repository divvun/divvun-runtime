#ifdef __cplusplus
extern "C" {
#endif

#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdbool.h>

#pragma once

#ifndef __APPLE__
#define _Nonnull
#define _Nullable
#endif

typedef uintptr_t rust_usize_t;

typedef struct rust_slice_s {
    void *_Nullable data;
    rust_usize_t len;
} rust_slice_t;

rust_slice_t dr__slice__new_str(const char *_Nonnull str) {
    size_t len = strlen(str);
    return (rust_slice_t){ .data = str, .len = len };
}

typedef struct rust_trait_object_s {
    rust_usize_t data;
    rust_usize_t vtable;
} rust_trait_object_t;

#define ERR_CALLBACK void (*_Nonnull exception)(char *_Nullable, rust_usize_t)

typedef void bundle_t;


void dr__rt__shutdown();

void dr__bundle__drop(bundle_t *_Nonnull bundle);

bundle_t *_Nonnull dr__bundle__from_bundle(
    rust_slice_t bundle_path,
    ERR_CALLBACK
);

bundle_t *_Nonnull dr__bundle__from_path(
    rust_slice_t path,
    ERR_CALLBACK
);

rust_slice_t dr__bundle__run_pipeline_bytes(
    bundle_t *_Nonnull bundle,
    rust_slice_t input,
    ERR_CALLBACK
);

rust_slice_t dr__bundle__run_pipeline_json(
    bundle_t *_Nonnull bundle,
    rust_slice_t input,
    ERR_CALLBACK
);

#ifdef __cplusplus
}
#endif