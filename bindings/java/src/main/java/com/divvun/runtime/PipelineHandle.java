package com.divvun.runtime;

import com.sun.jna.Pointer;
import com.divvun.runtime.util.ErrorCallback;
import com.divvun.runtime.util.RustSlice;

import java.nio.charset.StandardCharsets;

/**
 * Handle for a pipeline instance that can process input through the configured pipeline.
 * Implements AutoCloseable for proper resource management.
 */
public class PipelineHandle implements AutoCloseable {
    private Pointer ptr;
    private boolean disposed = false;

    /**
     * Create a PipelineHandle from a native pointer.
     * This constructor is package-private and should only be called by Bundle.
     *
     * @param ptr Native pointer to the pipeline handle
     */
    PipelineHandle(Pointer ptr) {
        this.ptr = ptr;
    }

    /**
     * Process input through the pipeline.
     *
     * @param input The input string to process
     * @return PipelineResponse containing the processed output
     * @throws IllegalStateException if the pipeline has been disposed
     * @throws RuntimeException if processing fails
     */
    public PipelineResponse forward(String input) {
        if (disposed || ptr == null) {
            throw new IllegalStateException("Pipeline has been disposed");
        }

        byte[] inputBytes = input.getBytes(StandardCharsets.UTF_8);
        RustSlice inputSlice = new RustSlice(inputBytes);

        try {
            RustSlice outputSlice = DivvunRuntimeLibrary.INSTANCE.DRT_PipelineHandle_forward(
                ptr,
                inputSlice,
                ErrorCallback.DEFAULT
            );
            return new PipelineResponse(outputSlice);
        } catch (Exception e) {
            throw new RuntimeException("Pipeline processing failed: " + e.getMessage(), e);
        }
    }

    /**
     * Close and dispose of the pipeline handle, releasing native resources.
     */
    @Override
    public void close() {
        if (!disposed && ptr != null) {
            DivvunRuntimeLibrary.INSTANCE.DRT_PipelineHandle_drop(ptr);
            disposed = true;
            ptr = null;
        }
    }

    /**
     * Check if the pipeline handle has been disposed.
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