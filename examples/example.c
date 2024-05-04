#include "divvun_runtime.h"
#include <stdio.h>

static void err_handler(char *_Nullable msg, rust_usize_t len)
{
    printf("Error: %.*s\n", (int)len, msg);
}

int main()
{
    bundle_t *bundle = dr__bundle__from_path(dr__slice__new_str("../../../pipeline-examples/tts"), &err_handler);
    rust_slice_t slice = dr__bundle__run_pipeline_bytes(bundle, dr__slice__new_str("hello world"), &err_handler);
    printf("We're here. Len: %lu\n", slice.len);
    dr__bundle__drop(bundle);
    return 0;
}