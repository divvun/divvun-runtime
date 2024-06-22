function New-TemporaryDirectory {
    $parent = [System.IO.Path]::GetTempPath()
    [string] $name = [System.Guid]::NewGuid()
    New-Item -ItemType Directory -Path (Join-Path $parent $name) | Out-Null
    (Join-Path $parent $name)
}

$tmp = New-TemporaryDirectory
# $tmp = "out"

$env:CMAKE_TOOLCHAIN_FILE="C:\vcpkg\scripts\buildsystems\vcpkg.cmake"
$env:ARTIFACT_PATH="C:\Program Files\Python311"
$env:PYO3_CONFIG_FILE="$(pwd)\pyo3.txt" # "$tmp/pyo3-build-config-file.txt"
$env:VCPKGRS_DYNAMIC="1"
$env:VCPKG_ROOT="C:\vcpkg"

# $cpython_dist = "C:\users\Brendan\python-build-standalone\dist\cpython-3.11.7-x86_64-pc-windows-msvc-shared-pgo-20240225T1814.tar.zst"
# $cpython_dist_sha256 = "0e3f1908bfd71474d1ff23afa981c27e1a27f76fefd0cd699f86bc5c38c0d3ba"
# $cpython_dist = "C:\Users\brendan\python-build-standalone\dist\cpython-3.11.7-x86_64-pc-windows-msvc-static-noopt-20240225T1814.tar.zst"
# $cpython_dist_sha256 = "811133e4a6e4d919cb37086d3f40d86a87395637fae927273e4e2e8d8e425b41"

# echo $tmp
# pyoxidizer generate-python-embedding-artifacts --system-rust --dynamic $tmp $cpython_dist $cpython_dist_sha256
# cargo build -p divvun-runtime-cli --release -vv --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
cargo build --lib --release --features ffi,divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
rm -r $tmp