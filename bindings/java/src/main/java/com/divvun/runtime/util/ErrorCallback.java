package com.divvun.runtime.util;

import com.sun.jna.Callback;
import com.sun.jna.Pointer;

/**
 * Error callback interface for handling Rust error messages.
 * This callback is invoked when the Rust side encounters an error.
 */
public interface ErrorCallback extends Callback {
    void invoke(Pointer ptr, long len);

    /**
     * Default error callback that throws a RuntimeException with the error message.
     */
    ErrorCallback DEFAULT = new ErrorCallback() {
        @Override
        public void invoke(Pointer ptr, long len) {
            if (ptr == null || ptr == Pointer.NULL) {
                throw new RuntimeException("Unknown error");
            }

            byte[] messageBytes = ptr.getByteArray(0, (int) len);
            String message = new String(messageBytes, java.nio.charset.StandardCharsets.UTF_8);
            throw new RuntimeException(message);
        }
    };
}