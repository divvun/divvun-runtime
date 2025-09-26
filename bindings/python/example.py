#!/usr/bin/env python3
"""
Example usage of Divvun Runtime Python bindings.

This script demonstrates how to use the Python bindings to load and execute
linguistic processing pipelines.
"""

import sys
from pathlib import Path
from divvun_runtime import Bundle, set_lib_path, DivvunRuntimeError


def main():
    if len(sys.argv) < 2:
        print("Usage: python example.py <bundle_path> [input_text]")
        print("       python example.py <pipeline_path> [input_text]")
        sys.exit(1)

    bundle_path = sys.argv[1]
    input_text = sys.argv[2] if len(sys.argv) > 2 else "Hello, world!"

    try:
        # Optionally set the library path if needed
        # set_lib_path("/path/to/library/directory")

        # Determine if we're loading a bundle (.drb) or pipeline file
        if bundle_path.endswith('.drb'):
            print(f"Loading bundle from: {bundle_path}")
            with Bundle.from_bundle(bundle_path) as bundle:
                process_with_bundle(bundle, input_text)
        else:
            print(f"Loading pipeline from: {bundle_path}")
            with Bundle.from_path(bundle_path) as bundle:
                process_with_bundle(bundle, input_text)

    except DivvunRuntimeError as e:
        print(f"Divvun Runtime Error: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)


def process_with_bundle(bundle: Bundle, input_text: str):
    """Process input text with the given bundle."""
    print(f"Input: {input_text}")

    # Create a pipeline with default configuration
    with bundle.create() as pipeline:
        # Process the input text
        with pipeline.forward(input_text) as response:
            # Get the result as a string
            result = response.string()
            print(f"Output: {result}")

    # Example with custom configuration
    print("\n--- Processing with custom configuration ---")
    config = {"debug": True}  # Example configuration

    with bundle.create(config) as pipeline:
        with pipeline.forward(input_text) as response:
            # You can also get the result as bytes or JSON
            try:
                json_result = response.json()
                print(f"JSON Output: {json_result}")
            except:
                # If not JSON, get as string
                string_result = response.string()
                print(f"String Output: {string_result}")


def demonstrate_multiple_inputs():
    """Demonstrate processing multiple inputs with the same pipeline."""
    inputs = ["First input", "Second input", "Third input"]

    # This would require a valid bundle path
    # bundle_path = "path/to/your/bundle.drb"

    print("--- Processing multiple inputs ---")
    # with Bundle.from_bundle(bundle_path) as bundle:
    #     with bundle.create() as pipeline:
    #         for i, text in enumerate(inputs, 1):
    #             with pipeline.forward(text) as response:
    #                 result = response.string()
    #                 print(f"Input {i}: {text} -> {result}")


if __name__ == "__main__":
    main()