#!/bin/sh
# The core crate must have no normal or build dependencies (SPEC §2).
set -eu
deps=$(cargo tree -p freeqrent --all-features -e normal,build --prefix none | sed 1d | sort -u)
if [ -n "$deps" ]; then
    echo "freeqrent has dependencies, which SPEC §2 forbids:" >&2
    echo "$deps" >&2
    exit 1
fi
echo "freeqrent: zero dependencies"
