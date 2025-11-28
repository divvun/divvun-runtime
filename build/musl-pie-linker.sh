#!/bin/sh
# Linker wrapper for musl builds that need dynamic GTK.
# Injects -Bdynamic before -l flags so system libs link dynamically.

# Build new args, injecting -Wl,-Bdynamic before first -l flag
injected=0
newargs=""
for arg in "$@"; do
  case "$arg" in
    -l*)
      if [ "$injected" = 0 ]; then
        newargs="$newargs -Wl,-Bdynamic"
        injected=1
      fi
      ;;
  esac
  newargs="$newargs $arg"
done

# Add libgcc_s for exception handling symbols
exec /usr/bin/gcc $newargs -lgcc_s
