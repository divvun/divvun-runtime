#!/bin/bash
# Linker wrapper for musl builds that need dynamic GTK.
# Injects -Bdynamic before -l flags so system libs link dynamically
# while Rust libs (.rlib) stay static.
args=()
found_first_l=false
for arg in "$@"; do
  # Insert -Bdynamic right before the first -l flag
  if [[ "$arg" == -l* ]] && [ "$found_first_l" = false ]; then
    args+=("-Wl,-Bdynamic")
    found_first_l=true
  fi
  args+=("$arg")
done
exec cc "${args[@]}"
