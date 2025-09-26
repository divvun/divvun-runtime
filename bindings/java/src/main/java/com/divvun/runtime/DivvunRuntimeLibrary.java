package com.divvun.runtime;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.Platform;
import com.divvun.runtime.util.ErrorCallback;
import com.divvun.runtime.util.RustSlice;

/**
 * JNA interface for the Divvun Runtime native library.
 * Maps to the FFI functions defined in src/ffi.rs
 */
public interface DivvunRuntimeLibrary extends Library {

    /**
     * Get the singleton instance of the library.
     */
    DivvunRuntimeLibrary INSTANCE = Native.load(getLibraryName(), DivvunRuntimeLibrary.class);

    /**
     * Create a Bundle from a bundle file path.
     *
     * @param bundlePath RustSlice containing the bundle path string
     * @param errorCallback Callback for error handling
     * @return Pointer to the created Bundle
     */
    Pointer DRT_Bundle_fromBundle(RustSlice bundlePath, ErrorCallback errorCallback);

    /**
     * Create a Bundle from a pipeline path.
     *
     * @param path RustSlice containing the pipeline path string
     * @param errorCallback Callback for error handling
     * @return Pointer to the created Bundle
     */
    Pointer DRT_Bundle_fromPath(RustSlice path, ErrorCallback errorCallback);

    /**
     * Create a PipelineHandle from a Bundle.
     *
     * @param bundle Pointer to the Bundle
     * @param config RustSlice containing the JSON configuration string
     * @param errorCallback Callback for error handling
     * @return Pointer to the created PipelineHandle
     */
    Pointer DRT_Bundle_create(Pointer bundle, RustSlice config, ErrorCallback errorCallback);

    /**
     * Drop/deallocate a Bundle.
     *
     * @param bundle Pointer to the Bundle to drop
     */
    void DRT_Bundle_drop(Pointer bundle);

    /**
     * Drop/deallocate a PipelineHandle.
     *
     * @param handle Pointer to the PipelineHandle to drop
     */
    void DRT_PipelineHandle_drop(Pointer handle);

    /**
     * Process input through a pipeline.
     *
     * @param pipe Pointer to the PipelineHandle
     * @param input RustSlice containing the input string
     * @param errorCallback Callback for error handling
     * @return RustSlice containing the output data
     */
    RustSlice DRT_PipelineHandle_forward(Pointer pipe, RustSlice input, ErrorCallback errorCallback);

    /**
     * Drop/deallocate a Vec<u8> returned from Rust.
     *
     * @param vec RustSlice representing the Vec<u8> to drop
     */
    void DRT_Vec_drop(RustSlice vec);

    /**
     * Get the platform-specific library name.
     */
    static String getLibraryName() {
        String libName = "divvun_runtime";

        if (Platform.isWindows()) {
            return "lib" + libName;
        } else if (Platform.isMac()) {
            return "lib" + libName;
        } else {
            return "lib" + libName;
        }
    }
}