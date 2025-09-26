package com.divvun.runtime;

import com.divvun.runtime.util.RustSlice;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.IOException;
import java.nio.charset.StandardCharsets;

/**
 * Response wrapper for pipeline execution results.
 * Provides methods to extract the response data as bytes, string, or JSON.
 * Implements AutoCloseable for proper resource management.
 */
public class PipelineResponse implements AutoCloseable {
    private RustSlice slice;
    private boolean disposed = false;
    private static final ObjectMapper objectMapper = new ObjectMapper();

    /**
     * Create a PipelineResponse from a RustSlice.
     *
     * @param slice The RustSlice containing the response data
     */
    public PipelineResponse(RustSlice slice) {
        this.slice = slice;
    }

    /**
     * Get the response data as a byte array.
     * This method disposes the response after returning the data.
     *
     * @return The response data as bytes
     * @throws IllegalStateException if the response has been disposed
     */
    public byte[] bytes() {
        if (disposed) {
            throw new IllegalStateException("Response has been disposed");
        }

        try {
            return slice.toByteArray();
        } finally {
            close();
        }
    }

    /**
     * Get the response data as a UTF-8 string.
     * This method disposes the response after returning the data.
     *
     * @return The response data as a string
     * @throws IllegalStateException if the response has been disposed
     */
    public String string() {
        if (disposed) {
            throw new IllegalStateException("Response has been disposed");
        }

        try {
            byte[] data = slice.toByteArray();
            return new String(data, StandardCharsets.UTF_8);
        } finally {
            close();
        }
    }

    /**
     * Parse the response data as JSON and return the parsed object.
     * This method disposes the response after returning the data.
     *
     * @return The parsed JSON object
     * @throws IllegalStateException if the response has been disposed
     * @throws RuntimeException if JSON parsing fails
     */
    public Object json() {
        String jsonString = string();
        try {
            return objectMapper.readValue(jsonString, Object.class);
        } catch (IOException e) {
            throw new RuntimeException("Failed to parse JSON response: " + e.getMessage(), e);
        }
    }

    /**
     * Parse the response data as JSON and return the parsed object of the specified type.
     *
     * @param valueType The class of the type to parse the JSON into
     * @param <T> The type to parse the JSON into
     * @return The parsed JSON object
     * @throws IllegalStateException if the response has been disposed
     * @throws RuntimeException if JSON parsing fails
     */
    public <T> T json(Class<T> valueType) {
        String jsonString = string();
        try {
            return objectMapper.readValue(jsonString, valueType);
        } catch (IOException e) {
            throw new RuntimeException("Failed to parse JSON response: " + e.getMessage(), e);
        }
    }

    /**
     * Close and dispose of the response, releasing any native resources.
     */
    @Override
    public void close() {
        if (!disposed && slice != null) {
            DivvunRuntimeLibrary.INSTANCE.DRT_Vec_drop(slice);
            disposed = true;
            slice = null;
        }
    }

    /**
     * Check if the response has been disposed.
     *
     * @return true if disposed, false otherwise
     */
    public boolean isDisposed() {
        return disposed;
    }
}