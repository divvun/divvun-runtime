#!/bin/sh
# Linker wrapper for musl builds that need dynamic GTK.
# Replaces -static-pie with -pie to allow dynamic library loading
# while keeping musl libc statically linked.
exec cc $(echo "$@" | sed 's/-static-pie/-pie/g')
