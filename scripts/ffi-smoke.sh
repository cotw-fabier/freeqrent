#!/bin/sh
# Builds freeqrent-ffi as a static library and links a C program against
# include/freeqrent.h with strict warnings, then runs it (SPEC §10.2).
set -eu
cd "$(dirname "$0")/.."
target=$(cargo metadata --format-version 1 --no-deps | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')
# The system libraries a Rust static library needs on this platform.
libs=$(cargo rustc -q -p freeqrent-ffi --release --crate-type staticlib -- --print native-static-libs 2>&1 \
    | sed -n 's/.*native-static-libs: //p' | tail -1)
version=$(cargo metadata --format-version 1 --no-deps | sed 's/.*"name":"freeqrent-ffi","version":"\([^"]*\)".*/\1/')
out="$target/ffi-smoke"
mkdir -p "$out"
${CC:-cc} -std=c99 -Wall -Wextra -Werror -pedantic \
    -DFREEQRENT_EXPECTED_VERSION="\"$version\"" \
    -I crates/freeqrent-ffi/include \
    crates/freeqrent-ffi/tests/smoke.c \
    "$target/release/libfreeqrent_ffi.a" $libs \
    -o "$out/smoke"
"$out/smoke"
