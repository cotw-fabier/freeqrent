#!/bin/sh
# Keeps the language bindings (SPEC §10) in step with the library. Run it
# whenever the core, freeqrent-doc, freeqrent-ffi, the schema or a binding
# changes (AGENTS.md):
#
#   scripts/bindings.sh generate          regenerate everything generated from
#                                         the library: the schema and the
#                                         conformance suite, each binding's typed
#                                         styles, and the Flutter goldens
#   scripts/bindings.sh test [LANG...]    build each binding and run its tests
#                                         (js python go dart; default all)
#   scripts/bindings.sh build [DIR]       the release artifacts, with SHA256SUMS
#                                         (default <cargo target dir>/release-<version>)
#   scripts/bindings.sh version X.Y.Z     set the version everywhere, then generate
#   scripts/bindings.sh all               generate, test, build
#
# `generate` leaves the tree as the checks expect it: if `git status` shows a
# change afterwards, the library changed something the bindings see, so
# review it like a snapshot and commit it with the change.
#
# Tools: Rust, Node ≥ 18, Python ≥ 3.9, Go, Dart and Flutter, and
# wasm-bindgen-cli at the version in bindings/js/crate/Cargo.lock. The
# script sets up each binding's own dependencies on first use (npm ci, a
# venv at bindings/python/.venv, pub get).
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)
target=$(cargo metadata --format-version 1 --no-deps | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
host=$(rustc -vV | sed -n 's/^host: //p')

say() { printf '\n== %s\n' "$*"; }

need() {
    command -v "$1" >/dev/null 2>&1 || { echo "bindings.sh: $1 not found ($2)" >&2; exit 1; }
}

js_setup() {
    need node "Node >= 18"
    need wasm-bindgen "cargo install wasm-bindgen-cli --locked --version $(wasm_bindgen_version)"
    if [ "$(wasm-bindgen --version | sed 's/wasm-bindgen //')" != "$(wasm_bindgen_version)" ]; then
        echo "bindings.sh: wasm-bindgen-cli must be $(wasm_bindgen_version)" >&2
        exit 1
    fi
    [ -d bindings/js/node_modules ] || (cd bindings/js && npm ci --no-audit --no-fund)
}

wasm_bindgen_version() {
    sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"/\1/p;}' bindings/js/crate/Cargo.lock
}

py_setup() {
    if [ ! -x bindings/python/.venv/bin/python ]; then
        python3 -m venv bindings/python/.venv
        bindings/python/.venv/bin/pip install --quiet maturin==1.15.0 pytest==9.1.1 mypy==2.4.0
    fi
}

dart_setup() {
    need dart "the Dart SDK"
    need flutter "the Flutter SDK"
    (cd bindings/dart/freeqrent && dart pub get >/dev/null)
    (cd bindings/dart/freeqrent_flutter && flutter pub get >/dev/null)
}

generate() {
    say "schema and conformance suite"
    cargo run -q -p freeqrent-doc --example generate
    say "npm types"
    node bindings/js/scripts/gen-types.mjs
    say "Python styles"
    python3 bindings/python/tools/generate_styles.py
    say "Go styles"
    (cd bindings/go && go generate ./...)
    say "Dart styles"
    (cd bindings/dart/freeqrent && dart pub get >/dev/null && dart run tool/generate.dart)
    say "Flutter goldens (resvg)"
    cargo run -q --release --manifest-path bindings/dart/freeqrent_flutter/goldens/Cargo.toml
}

test_js() {
    js_setup
    say "npm: build and test"
    cargo build -q -p freeqrent-cli
    (cd bindings/js && npm run -s build && FREEQRENT_CLI="$target/debug/freeqrent" npm test)
}

test_python() {
    py_setup
    say "Python: build and test"
    (cd bindings/python && .venv/bin/maturin develop --release -q && .venv/bin/python -m pytest -q)
}

test_go() {
    need go "Go"
    say "Go: libraries for $host and wasm32"
    scripts/build-prebuilt.sh --go "$host" wasm32-unknown-unknown >/dev/null
    say "Go: cgo and wazero builds"
    (cd bindings/go && go vet ./... && go test -count=1 ./... \
        && CGO_ENABLED=0 go vet -tags freeqrent_wasm ./... \
        && CGO_ENABLED=0 go test -count=1 -tags freeqrent_wasm ./...)
}

test_dart() {
    dart_setup
    say "Dart and Flutter: test"
    (cd bindings/dart/freeqrent && dart analyze --fatal-infos && dart test)
    (cd bindings/dart/freeqrent_flutter && flutter analyze --fatal-infos && flutter test)
}

run_tests() {
    langs=${*:-js python go dart}
    for lang in $langs; do
        case $lang in
            js) test_js ;;
            python) test_python ;;
            go) test_go ;;
            dart) test_dart ;;
            *) echo "bindings.sh: unknown binding $lang (js python go dart)" >&2; exit 2 ;;
        esac
    done
}

build() {
    out=${1:-$target/release-$version}
    case $out in /*) ;; *) out="$root/$out" ;; esac
    mkdir -p "$out"
    say "C ABI libraries for every installed target"
    PREBUILT_OUT="$target/prebuilt" scripts/build-prebuilt.sh --package
    for f in "$target/prebuilt/dist/"freeqrent-ffi-"$version"-*; do
        case $f in *.sha256) ;; *) cp "$f" "$out/" ;; esac
    done
    say "npm package"
    js_setup
    (cd bindings/js && npm run -s build && npm pack --silent --pack-destination "$out")
    say "Python wheel and sdist"
    py_setup
    wheel_target=""
    [ "$(uname -s)" = Darwin ] && wheel_target="--target universal2-apple-darwin"
    # shellcheck disable=SC2086
    (cd bindings/python && .venv/bin/maturin build --release -q $wheel_target --out "$out" \
        && .venv/bin/maturin sdist -q --out "$out")
    say "checksums"
    (cd "$out" && rm -f SHA256SUMS && shasum -a 256 -- * > SHA256SUMS && cat SHA256SUMS)
    echo "release artifacts: $out"
}

set_version() {
    new=$1
    echo "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "bindings.sh: $new isn't X.Y.Z" >&2; exit 2; }
    old=$version
    say "version $old -> $new"
    # The workspace and the path dependencies' version requirements.
    sed -i.bak "s/^version = \"$old\"/version = \"$new\"/" Cargo.toml
    for f in crates/*/Cargo.toml; do
        sed -i.bak "s/\(path = \"[^\"]*\", version = \)\"$old\"/\1\"$new\"/" "$f"
    done
    # Each binding's own manifests.
    for f in bindings/js/crate/Cargo.toml bindings/python/Cargo.toml bindings/python/pyproject.toml \
        bindings/dart/freeqrent/rust/Cargo.toml bindings/dart/freeqrent_flutter/goldens/Cargo.toml; do
        sed -i.bak "s/^version = \"$old\"/version = \"$new\"/" "$f"
    done
    for f in bindings/dart/freeqrent/pubspec.yaml bindings/dart/freeqrent_flutter/pubspec.yaml; do
        sed -i.bak "s/^version: $old$/version: $new/" "$f"
    done
    for f in Cargo.toml crates/*/Cargo.toml bindings/js/crate/Cargo.toml bindings/python/Cargo.toml \
        bindings/python/pyproject.toml bindings/dart/freeqrent/rust/Cargo.toml \
        bindings/dart/freeqrent_flutter/goldens/Cargo.toml bindings/dart/freeqrent/pubspec.yaml \
        bindings/dart/freeqrent_flutter/pubspec.yaml; do
        rm "$f.bak"
    done
    (cd bindings/js && npm version "$new" --no-git-tag-version --allow-same-version >/dev/null)
    cargo update -q -w
    for m in bindings/js/crate bindings/python bindings/dart/freeqrent/rust bindings/dart/freeqrent_flutter/goldens; do
        cargo update -q -w --manifest-path "$m/Cargo.toml"
    done
    version=$new
    generate
}

case ${1:-} in
    generate) generate ;;
    test) shift; run_tests "$@" ;;
    build) shift; build "$@" ;;
    version) [ $# -eq 2 ] || { echo "usage: scripts/bindings.sh version X.Y.Z" >&2; exit 2; }; set_version "$2" ;;
    all) generate; run_tests; build ;;
    *) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
