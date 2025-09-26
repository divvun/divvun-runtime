"""
Divvun Runtime Python Bindings

This package provides Python bindings for the Divvun Runtime library,
allowing you to process linguistic data through configurable pipelines.

Example usage:

    from divvun_runtime import Bundle

    # Load a bundle from a .drb file
    with Bundle.from_bundle("path/to/bundle.drb") as bundle:
        # Create a pipeline with optional configuration
        with bundle.create() as pipeline:
            # Process input text
            with pipeline.forward("input text") as response:
                result = response.string()
                print(result)
"""

from .divvun_runtime import (
    Bundle,
    PipelineHandle,
    PipelineResponse,
    DivvunRuntimeError,
    set_lib_path,
)

__all__ = [
    'Bundle',
    'PipelineHandle',
    'PipelineResponse',
    'DivvunRuntimeError',
    'set_lib_path',
]

__version__ = "0.1.0"