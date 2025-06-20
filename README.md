# Divvun Runtime

This basically will not build on any computer but mine right now. This is to be fixed very soon.

## Prereqs

- libtorch 2.4.1 with C++11 ABI
	- download [this file](https://download.pytorch.org/libtorch/cpu/libtorch-macos-arm64-2.7.1.zip) (or go check for a newer version)
	- pop it into `/opt/libtorch`

## Building

You'll need `just`. Then run `just build-cli`.
