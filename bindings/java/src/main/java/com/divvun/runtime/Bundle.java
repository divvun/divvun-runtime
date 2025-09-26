package com.divvun.runtime;

import com.sun.jna.Pointer;
import com.divvun.runtime.util.ErrorCallback;
import com.divvun.runtime.util.RustSlice;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.nio.charset.StandardCharsets;
import java.util.Map;

/**
 * Bundle represents a pipeline bundle that can be used to create pipeline handles.
 * Implements AutoCloseable for proper resource management.
 */
public class Bundle implements AutoCloseable {
    private Pointer ptr;
    private boolean disposed = false;
    private static final ObjectMapper objectMapper = new ObjectMapper();

    /**
     * Create a Bundle from a pipeline path.
     *
     * @param pipelinePath Path to the pipeline configuration
     * @return A new Bundle instance
     * @throws RuntimeException if bundle creation fails
     */
    public static Bundle fromPath(String pipelinePath) {
        byte[] pathBytes = pipelinePath.getBytes(StandardCharsets.UTF_8);
        RustSlice pathSlice = new RustSlice(pathBytes);

        try {
            Pointer bundlePtr = DivvunRuntimeLibrary.INSTANCE.DRT_Bundle_fromPath(
                pathSlice,
                ErrorCallback.DEFAULT
            );

            if (bundlePtr == null) {
                throw new RuntimeException("Failed to create bundle from path: " + pipelinePath);
            }

            return new Bundle(bundlePtr);
        } catch (Exception e) {
            throw new RuntimeException("Bundle creation from path failed: " + e.getMessage(), e);
        }
    }

    /**
     * Create a Bundle from a bundle file.
     *
     * @param bundlePath Path to the bundle file
     * @return A new Bundle instance
     * @throws RuntimeException if bundle creation fails
     */
    public static Bundle fromBundle(String bundlePath) {
        byte[] pathBytes = bundlePath.getBytes(StandardCharsets.UTF_8);
        RustSlice pathSlice = new RustSlice(pathBytes);

        try {
            Pointer bundlePtr = DivvunRuntimeLibrary.INSTANCE.DRT_Bundle_fromBundle(
                pathSlice,
                ErrorCallback.DEFAULT
            );

            if (bundlePtr == null) {
                throw new RuntimeException("Failed to create bundle from bundle file: " + bundlePath);
            }

            return new Bundle(bundlePtr);
        } catch (Exception e) {
            throw new RuntimeException("Bundle creation from bundle failed: " + e.getMessage(), e);
        }
    }

    /**
     * Private constructor for creating a Bundle from a native pointer.
     *
     * @param ptr Native pointer to the bundle
     */
    private Bundle(Pointer ptr) {
        this.ptr = ptr;
    }

    /**
     * Create a pipeline handle with the given configuration.
     *
     * @param config Configuration map for the pipeline (defaults to empty if null)
     * @return A new PipelineHandle instance
     * @throws IllegalStateException if the bundle has been disposed
     * @throws RuntimeException if pipeline creation fails
     */
    public PipelineHandle create(Map<String, Object> config) {
        if (disposed || ptr == null) {
            throw new IllegalStateException("Bundle has been disposed");
        }

        try {
            String configJson = "{}";
            if (config != null && !config.isEmpty()) {
                configJson = objectMapper.writeValueAsString(config);
            }

            byte[] configBytes = configJson.getBytes(StandardCharsets.UTF_8);
            RustSlice configSlice = new RustSlice(configBytes);

            Pointer pipelinePtr = DivvunRuntimeLibrary.INSTANCE.DRT_Bundle_create(
                ptr,
                configSlice,
                ErrorCallback.DEFAULT
            );

            if (pipelinePtr == null) {
                throw new RuntimeException("Failed to create pipeline handle");
            }

            return new PipelineHandle(pipelinePtr);
        } catch (Exception e) {
            throw new RuntimeException("Pipeline creation failed: " + e.getMessage(), e);
        }
    }

    /**
     * Create a pipeline handle with default configuration.
     *
     * @return A new PipelineHandle instance
     * @throws IllegalStateException if the bundle has been disposed
     * @throws RuntimeException if pipeline creation fails
     */
    public PipelineHandle create() {
        return create(null);
    }

    /**
     * Close and dispose of the bundle, releasing native resources.
     */
    @Override
    public void close() {
        if (!disposed && ptr != null) {
            DivvunRuntimeLibrary.INSTANCE.DRT_Bundle_drop(ptr);
            disposed = true;
            ptr = null;
        }
    }

    /**
     * Check if the bundle has been disposed.
     *
     * @return true if disposed, false otherwise
     */
    public boolean isDisposed() {
        return disposed;
    }

    /**
     * Finalize method to ensure resources are cleaned up if close() wasn't called.
     */
    @Override
    protected void finalize() throws Throwable {
        try {
            close();
        } finally {
            super.finalize();
        }
    }
}