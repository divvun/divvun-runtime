def make_exe():
    dist = PythonDistribution(
        sha256="00a56ae1a89edb35a2c77bdcd26f3061302ddc17b025ea54a447b98638b9a92b",
        local_path="/Users/brendan/git/necessary/python-build-standalone/dist/cpython-3.11.7-aarch64-apple-darwin-noopt-20240206T1427.tar.zst"
        # sha256="47b7945e417e2d3c65dd049287107dc79e0cfc49290b01c2db2443c0aaf47e7f",
        # local_path="/Users/brendan/git/necessary/python-build-standalone/dist/cpython-3.11.7-aarch64-apple-darwin-pgo+lto-20240206T1427.tar.zst"
    )

    policy = dist.make_python_packaging_policy()
    policy.extension_module_filter = "all"
    policy.bytecode_optimize_level_two = True
    policy.include_distribution_resources = True
    policy.resources_location = "filesystem-relative:lib/python3.11"
    python_config = dist.make_python_interpreter_config()
    python_config.config_profile = "isolated"
    python_config.allocator_backend = "default"
    python_config.allocator_debug = False
    python_config.oxidized_importer = False
    python_config.filesystem_importer = True
    python_config.sys_frozen = True

    # policy.resources_location = "filesystem-relative:lib/python3.11/site-packages"

    exe = dist.to_python_executable(
        name="divvun-runtime",
        packaging_policy=policy,
        config=python_config,
    )

    # Return our `PythonExecutable` instance so it can be built and
    # referenced by other consumers of this target.
    return exe

def make_embedded_resources(exe):
    return exe.to_embedded_resources()

def make_install(exe):
    # Create an object that represents our installed application file layout.
    files = FileManifest()

    # Add the generated executable to our install layout in the root directory.
    files.add_python_resource(".", exe)

    return files

# Tell PyOxidizer about the build targets defined above.
register_target("exe", make_exe)
register_target("resources", make_embedded_resources, depends=["exe"], default_build_script=True)
register_target("install", make_install, depends=["exe"], default=True)

# Resolve whatever targets the invoker of this configuration file is requesting
# be resolved.
resolve_targets()
