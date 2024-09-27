install_name_tool -change /opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11/Python @executable_path/libpython3.11.dylib ./target/release/divvun-runtime-cli
cp /opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11/Python target/release/libpython3.11.dylib

