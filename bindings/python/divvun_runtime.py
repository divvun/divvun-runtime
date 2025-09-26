import ctypes
import json
import platform
from ctypes import Structure, c_void_p
from pathlib import Path
from typing import Dict, Any, Optional
import struct

_BITNESS = struct.calcsize("P") * 8

rust_usize_t = ctypes.c_uint64 if _BITNESS == 64 else ctypes.c_uint32

class RustSlice(Structure):
    _fields_ = [("ptr", c_void_p), ("len", rust_usize_t)]


class DivvunRuntimeError(Exception):
    pass


_lib: Optional[ctypes.CDLL] = None


def _load_library():
    global _lib
    if _lib is not None:
        return _lib

    system = platform.system()
    if system == "Windows":
        lib_name = "divvun_runtime.dll"
    elif system == "Darwin":
        lib_name = "libdivvun_runtime.dylib"
    else:
        lib_name = "libdivvun_runtime.so"

    try:
        _lib = _setup_function_signatures(ctypes.CDLL(lib_name))
    except OSError:
        raise DivvunRuntimeError(f"Could not load library {lib_name}")

    return _lib


def _make_rust_string(s: str) -> RustSlice:
    encoded = s.encode('utf-8')
    ptr = ctypes.cast(ctypes.c_char_p(encoded), c_void_p)
    return RustSlice(ptr, len(encoded))


ERROR_CALLBACK_TYPE = ctypes.CFUNCTYPE(None, c_void_p, rust_usize_t)


def _error_callback(ptr: c_void_p, length: ctypes.c_uint64 | ctypes.c_uint32):
    if ptr:
        message_bytes = ctypes.string_at(ptr, length.value)
        message = message_bytes.decode('utf-8')
        raise DivvunRuntimeError(message)
    else:
        raise DivvunRuntimeError("Unknown error")


_error_callback_instance = ERROR_CALLBACK_TYPE(_error_callback)


def _setup_function_signatures(lib: ctypes.CDLL) -> ctypes.CDLL:
    lib.DRT_Bundle_fromBundle.argtypes = [RustSlice, ERROR_CALLBACK_TYPE]
    lib.DRT_Bundle_fromBundle.restype = c_void_p

    lib.DRT_Bundle_fromPath.argtypes = [RustSlice, ERROR_CALLBACK_TYPE]
    lib.DRT_Bundle_fromPath.restype = c_void_p

    lib.DRT_Bundle_create.argtypes = [c_void_p, RustSlice, ERROR_CALLBACK_TYPE]
    lib.DRT_Bundle_create.restype = c_void_p

    lib.DRT_Bundle_drop.argtypes = [c_void_p]
    lib.DRT_Bundle_drop.restype = None

    lib.DRT_PipelineHandle_drop.argtypes = [c_void_p]
    lib.DRT_PipelineHandle_drop.restype = None

    lib.DRT_PipelineHandle_forward.argtypes = [c_void_p, RustSlice, ERROR_CALLBACK_TYPE]
    lib.DRT_PipelineHandle_forward.restype = RustSlice

    lib.DRT_Vec_drop.argtypes = [RustSlice]
    lib.DRT_Vec_drop.restype = None

    return lib


class PipelineResponse:
    def __init__(self, rust_slice: RustSlice):
        self._slice = rust_slice
        self._disposed = False

        if rust_slice.ptr:
            self._data = ctypes.string_at(rust_slice.ptr, rust_slice.len)
        else:
            self._data = b""

    def __del__(self):
        self._dispose()

    def __enter__(self):
        return self

    def __exit__(self, _exc_type, _exc_val, _exc_tb):
        self._dispose()

    def _dispose(self):
        if not self._disposed and self._slice.ptr:
            lib = _load_library()
            lib.DRT_Vec_drop(self._slice)
            self._disposed = True

    def bytes(self) -> bytes:
        if self._disposed:
            raise DivvunRuntimeError("Response has been disposed")
        result = self._data
        self._dispose()
        return result

    def string(self) -> str:
        return self.bytes().decode('utf-8')

    def json(self) -> Any:
        return json.loads(self.string())


class PipelineHandle:
    def __init__(self, ptr: c_void_p):
        self._ptr = ptr
        self._disposed = False

    def __del__(self):
        self._dispose()

    def __enter__(self):
        return self

    def __exit__(self, _exc_type, _exc_val, _exc_tb):
        self._dispose()

    def _dispose(self):
        if not self._disposed and self._ptr:
            lib = _load_library()
            lib.DRT_PipelineHandle_drop(self._ptr)
            self._ptr = None
            self._disposed = True

    def forward(self, input_text: str) -> PipelineResponse:
        if self._disposed:
            raise DivvunRuntimeError("Pipeline has been disposed")

        lib = _load_library()
        rust_input = _make_rust_string(input_text)

        try:
            output_slice = lib.DRT_PipelineHandle_forward(
                self._ptr,
                rust_input,
                _error_callback_instance
            )
            return PipelineResponse(output_slice)
        except Exception as e:
            raise e


class Bundle:
    def __init__(self, ptr: c_void_p):
        self._ptr = ptr
        self._disposed = False

    def __del__(self):
        self._dispose()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self._dispose()

    def _dispose(self):
        if not self._disposed and self._ptr:
            lib = _load_library()
            lib.DRT_Bundle_drop(self._ptr)
            self._ptr = None
            self._disposed = True

    @classmethod
    def from_path(cls, pipeline_path: str) -> 'Bundle':
        lib = _load_library()
        rust_path = _make_rust_string(pipeline_path)

        try:
            bundle_ptr = lib.DRT_Bundle_fromPath(rust_path, _error_callback_instance)
            if not bundle_ptr:
                raise DivvunRuntimeError("Failed to create bundle from path")
            return cls(bundle_ptr)
        except Exception as e:
            raise e

    @classmethod
    def from_bundle(cls, bundle_path: str) -> 'Bundle':
        lib = _load_library()
        rust_path = _make_rust_string(bundle_path)

        try:
            bundle_ptr = lib.DRT_Bundle_fromBundle(rust_path, _error_callback_instance)
            if not bundle_ptr:
                raise DivvunRuntimeError("Failed to create bundle from bundle")
            return cls(bundle_ptr)
        except Exception as e:
            raise e

    def create(self, config: Optional[Dict[str, Any]] = None) -> PipelineHandle:
        if self._disposed:
            raise DivvunRuntimeError("Bundle has been disposed")

        if config is None:
            config = {}

        lib = _load_library()
        config_str = json.dumps(config)
        rust_config = _make_rust_string(config_str)

        try:
            pipeline_ptr = lib.DRT_Bundle_create(
                self._ptr,
                rust_config,
                _error_callback_instance
            )
            if not pipeline_ptr:
                raise DivvunRuntimeError("Failed to create pipeline")
            return PipelineHandle(pipeline_ptr)
        except Exception as e:
            raise e


def set_lib_path(path: str):
    """Set the path where the library should be loaded from."""
    global _lib
    _lib = None  # Reset library so it can be reloaded from new path

    system = platform.system()
    if system == "Windows":
        lib_name = "divvun_runtime.dll"
    elif system == "Darwin":
        lib_name = "libdivvun_runtime.dylib"
    else:
        lib_name = "libdivvun_runtime.so"

    full_path = Path(path) / lib_name

    try:
        _lib = _setup_function_signatures(ctypes.CDLL(str(full_path)))
    except OSError:
        raise DivvunRuntimeError(f"Could not load library from {full_path}")