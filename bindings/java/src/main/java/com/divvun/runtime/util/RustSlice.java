package com.divvun.runtime.util;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

/**
 * JNA structure representing a Rust slice (pointer + length).
 * Maps to the Rust type { pointer: *const T, len: usize }
 */
@Structure.FieldOrder({"ptr", "len"})
public class RustSlice extends Structure {
    public Pointer ptr;
    public long len;

    public RustSlice() {
        super();
    }

    public RustSlice(Pointer ptr, long len) {
        this.ptr = ptr;
        this.len = len;
    }

    public RustSlice(byte[] data) {
        if (data != null && data.length > 0) {
            this.ptr = new com.sun.jna.Memory(data.length);
            this.ptr.write(0, data, 0, data.length);
            this.len = data.length;
        } else {
            this.ptr = Pointer.NULL;
            this.len = 0;
        }
    }

    public byte[] toByteArray() {
        if (ptr == null || ptr == Pointer.NULL || len == 0) {
            return new byte[0];
        }
        return ptr.getByteArray(0, (int) len);
    }

    @Override
    protected List<String> getFieldOrder() {
        return Arrays.asList("ptr", "len");
    }
}