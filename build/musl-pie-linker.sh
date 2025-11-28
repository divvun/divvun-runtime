#!/bin/bash
# Linker wrapper for musl builds that need dynamic GTK.
# Replaces -static-pie with -pie to allow dynamic library loading
# while keeping musl libc statically linked.
args=()
for arg in "$@"; do
  if [ "$arg" = "-static-pie" ]; then
    args+=("-pie")
  else
    args+=("$arg")
  fi
done
exec cc "${args[@]}"
