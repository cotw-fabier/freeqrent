#!/bin/sh
# The core crate must have no normal or build dependencies (SPEC §2), and
# the binding boundary (freeqrent-doc) none besides the core.
set -eu
deps=$(cargo tree -p freeqrent --all-features -e normal,build --prefix none | sed 1d | sort -u)
if [ -n "$deps" ]; then
    echo "freeqrent has dependencies, which SPEC §2 forbids:" >&2
    echo "$deps" >&2
    exit 1
fi
echo "freeqrent: zero dependencies"
deps=$(cargo tree -p freeqrent-doc -e normal,build --prefix none | sed 1d | grep -v '^freeqrent v' | sort -u)
if [ -n "$deps" ]; then
    echo "freeqrent-doc has dependencies besides freeqrent, which SPEC §2 forbids:" >&2
    echo "$deps" >&2
    exit 1
fi
echo "freeqrent-doc: no dependencies besides freeqrent"
