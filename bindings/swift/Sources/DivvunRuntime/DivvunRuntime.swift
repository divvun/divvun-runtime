import Foundation
import CDivvunRuntime

// MARK: - Error Handling

public struct DivvunRuntimeError: Error, LocalizedError {
    public let message: String

    public var errorDescription: String? {
        return message
    }
}

private class ErrorCapture {
    var error: DivvunRuntimeError?
}

// MARK: - Helper Functions

private func makeRustString(_ string: String) -> rust_slice_t {
    let data = string.data(using: .utf8)!
    return data.withUnsafeBytes { bytes in
        rust_slice_t(
            data: bytes.baseAddress,
            len: rust_usize_t(bytes.count)
        )
    }
}

private func rustSliceToData(_ slice: rust_slice_t) -> Data {
    guard let ptr = slice.data else {
        return Data()
    }
    return Data(bytes: ptr, count: Int(slice.len))
}

private func rustSliceToString(_ slice: rust_slice_t) -> String? {
    let data = rustSliceToData(slice)
    return String(data: data, encoding: .utf8)
}

// MARK: - Response Protocol

public protocol PipelineResponseConvertible {
    static func from(data: Data) throws -> Self
}

extension Data: PipelineResponseConvertible {
    public static func from(data: Data) throws -> Data {
        return data
    }
}

extension String: PipelineResponseConvertible {
    public static func from(data: Data) throws -> String {
        guard let string = String(data: data, encoding: .utf8) else {
            throw DivvunRuntimeError(message: "Failed to decode UTF-8 string from response")
        }
        return string
    }
}

extension Array: PipelineResponseConvertible where Element == String {
    public static func from(data: Data) throws -> [String] {
        let decoder = JSONDecoder()
        return try decoder.decode([String].self, from: data)
    }
}

extension Dictionary: PipelineResponseConvertible where Key == String, Value == Any {
    public static func from(data: Data) throws -> [String: Any] {
        let json = try JSONSerialization.jsonObject(with: data, options: [])
        guard let dict = json as? [String: Any] else {
            throw DivvunRuntimeError(message: "Response is not a JSON object")
        }
        return dict
    }
}

// Allow any Decodable type
extension PipelineResponseConvertible where Self: Decodable {
    public static func from(data: Data) throws -> Self {
        let decoder = JSONDecoder()
        return try decoder.decode(Self.self, from: data)
    }
}

// MARK: - Bundle

public class Bundle {
    private let handle: bundle_handle_t

    private init(handle: bundle_handle_t) {
        self.handle = handle
    }

    deinit {
        DRT_Bundle_drop(handle)
    }

    public static func fromPath(_ path: String) throws -> Bundle {
        let capture = ErrorCapture()
        let pathSlice = makeRustString(path)

        let errorCallback: error_callback_t = { errorPtr, errorLen in
            if let ptr = errorPtr {
                let slice = rust_slice_t(data: ptr, len: errorLen)
                if let message = rustSliceToString(slice) {
                    capture.error = DivvunRuntimeError(message: message)
                }
            } else {
                capture.error = DivvunRuntimeError(message: "Unknown error")
            }
        }

        guard let handle = DRT_Bundle_fromPath(pathSlice, errorCallback) else {
            throw capture.error ?? DivvunRuntimeError(message: "Failed to load bundle from path")
        }

        return Bundle(handle: handle)
    }

    public static func fromBundle(_ bundlePath: String) throws -> Bundle {
        let capture = ErrorCapture()
        let pathSlice = makeRustString(bundlePath)

        let errorCallback: error_callback_t = { errorPtr, errorLen in
            if let ptr = errorPtr {
                let slice = rust_slice_t(data: ptr, len: errorLen)
                if let message = rustSliceToString(slice) {
                    capture.error = DivvunRuntimeError(message: message)
                }
            } else {
                capture.error = DivvunRuntimeError(message: "Unknown error")
            }
        }

        guard let handle = DRT_Bundle_fromBundle(pathSlice, errorCallback) else {
            throw capture.error ?? DivvunRuntimeError(message: "Failed to load bundle")
        }

        return Bundle(handle: handle)
    }

    public func create(config: [String: Any] = [:]) throws -> PipelineHandle {
        let capture = ErrorCapture()

        let configData = try JSONSerialization.data(withJSONObject: config, options: [])
        let configString = String(data: configData, encoding: .utf8) ?? "{}"
        let configSlice = makeRustString(configString)

        let errorCallback: error_callback_t = { errorPtr, errorLen in
            if let ptr = errorPtr {
                let slice = rust_slice_t(data: ptr, len: errorLen)
                if let message = rustSliceToString(slice) {
                    capture.error = DivvunRuntimeError(message: message)
                }
            } else {
                capture.error = DivvunRuntimeError(message: "Unknown error")
            }
        }

        guard let pipelineHandle = DRT_Bundle_create(handle, configSlice, errorCallback) else {
            throw capture.error ?? DivvunRuntimeError(message: "Failed to create pipeline")
        }

        return PipelineHandle(handle: pipelineHandle)
    }
}

// MARK: - PipelineHandle

public class PipelineHandle {
    private let handle: pipeline_handle_t

    fileprivate init(handle: pipeline_handle_t) {
        self.handle = handle
    }

    deinit {
        DRT_PipelineHandle_drop(handle)
    }

    /// Forward input through the pipeline and get typed response
    public func forward<T: PipelineResponseConvertible>(_ input: String, as type: T.Type = Data.self) throws -> T {
        let capture = ErrorCapture()
        let inputSlice = makeRustString(input)

        let errorCallback: error_callback_t = { errorPtr, errorLen in
            if let ptr = errorPtr {
                let slice = rust_slice_t(data: ptr, len: errorLen)
                if let message = rustSliceToString(slice) {
                    capture.error = DivvunRuntimeError(message: message)
                }
            } else {
                capture.error = DivvunRuntimeError(message: "Unknown error")
            }
        }

        let outputSlice = DRT_PipelineHandle_forward(handle, inputSlice, errorCallback)

        if let error = capture.error {
            throw error
        }

        defer {
            // Clean up the Rust-allocated memory
            DRT_Vec_drop(outputSlice)
        }

        let data = rustSliceToData(outputSlice)
        return try T.from(data: data)
    }

    /// Convenience method for getting raw bytes
    public func forwardBytes(_ input: String) throws -> Data {
        return try forward(input, as: Data.self)
    }

    /// Convenience method for getting string response
    public func forwardString(_ input: String) throws -> String {
        return try forward(input, as: String.self)
    }

    /// Convenience method for getting JSON response as dictionary
    public func forwardJSON(_ input: String) throws -> [String: Any] {
        return try forward(input, as: [String: Any].self)
    }

    /// Convenience method for getting typed JSON response
    public func forwardJSON<T: Decodable>(_ input: String, as type: T.Type) throws -> T {
        let data = try forward(input, as: Data.self)
        let decoder = JSONDecoder()
        return try decoder.decode(T.self, from: data)
    }
}