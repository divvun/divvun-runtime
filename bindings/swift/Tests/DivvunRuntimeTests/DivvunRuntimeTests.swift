import XCTest
@testable import DivvunRuntime

final class DivvunRuntimeTests: XCTestCase {
    func testBundleCreation() throws {
        // This test would require an actual bundle file
        // Bundle.fromPath("/path/to/bundle.drb")
    }

    func testTypedResponses() throws {
        // Example of how the typed API would work:

        // For raw bytes:
        // let bytes: Data = try pipeline.forward("input", as: Data.self)

        // For string:
        // let text: String = try pipeline.forward("input", as: String.self)

        // For JSON dictionary:
        // let json: [String: Any] = try pipeline.forward("input", as: [String: Any].self)

        // For typed JSON:
        // struct MyResponse: Codable { let result: String }
        // let response: MyResponse = try pipeline.forward("input", as: MyResponse.self)

        // Convenience methods:
        // let bytes = try pipeline.forwardBytes("input")
        // let text = try pipeline.forwardString("input")
        // let json = try pipeline.forwardJSON("input")
        // let typed: MyResponse = try pipeline.forwardJSON("input", as: MyResponse.self)
    }
}

// Example of a custom response type
struct ExampleResponse: Codable {
    let text: String
    let confidence: Double
}