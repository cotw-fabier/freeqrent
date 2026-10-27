#!/bin/sh
# Builds freeqrent-ffi's prebuilt libraries (SPEC §10.3: "Prebuilt binaries
# are produced for every platform"), static and dynamic, with the header,
# one directory per Rust target, and optionally packages and checksums them.
#
#   scripts/build-prebuilt.sh [--go] [--package] [--strict] [TARGET...]
#
# With no TARGET it builds the host and every installed rustup target it
# knows (the list below). Output goes to $PREBUILT_OUT, by default
# <cargo target dir>/prebuilt:
#
#   <out>/<target>/include/freeqrent.h
#   <out>/<target>/lib/libfreeqrent_ffi.a        (freeqrent_ffi.lib on msvc)
#   <out>/<target>/lib/libfreeqrent_ffi.{so,dylib} / freeqrent_ffi.dll
#   <out>/<target>/native-static-libs.txt         what a static link needs
#   <out>/wasm32-unknown-unknown/freeqrent_ffi.wasm
#   <out>/universal-apple-darwin/...              lipo of both macOS slices
#   <out>/freeqrent-ffi.xcframework               iOS device + simulator
#
# --go      also copies the static libraries into bindings/go/lib/<goos>_<goarch>/
#           and the .wasm into bindings/go/wasm/, and checks that the link
#           flags in bindings/go/link_*.go cover what rustc says each
#           library needs.
# --package writes <out>/dist/freeqrent-ffi-<version>-<target>.tar.gz (zip
#           for windows) and a .sha256 beside each, plus SHA256SUMS.
# --strict  makes a library that can't be built (no linker for a cdylib, no
#           NDK) an error instead of a note. CI uses it.
#
# A cdylib needs a linker for its target; a staticlib doesn't. Android
# cdylibs use the NDK's clang from $ANDROID_NDK_HOME (or
# $ANDROID_NDK_LATEST_HOME, or the newest NDK under ~/Library/Android/sdk/ndk
# or ~/Android/Sdk/ndk), API level $ANDROID_API (default 21).
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

go=0
package=0
strict=0
targets=""
for arg in "$@"; do
    case "$arg" in
        --go) go=1 ;;
        --package) package=1 ;;
        --strict) strict=1 ;;
        -h|--help) sed -n '2,33p' "$0"; exit 0 ;;
        -*) echo "build-prebuilt: unknown option $arg" >&2; exit 2 ;;
        *) targets="$targets $arg" ;;
    esac
done

known="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
x86_64-apple-darwin aarch64-apple-darwin
x86_64-pc-windows-msvc aarch64-pc-windows-msvc
x86_64-pc-windows-gnu aarch64-pc-windows-gnullvm
aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
wasm32-unknown-unknown"

host=$(rustc -vV | sed -n 's/^host: //p')
if [ -z "$targets" ]; then
    installed=$(rustup target list --installed 2>/dev/null || echo "$host")
    for t in $known; do
        if [ "$t" = "$host" ] || echo "$installed" | grep -qx "$t"; then
            targets="$targets $t"
        fi
    done
fi

target_dir=$(cargo metadata --format-version 1 --no-deps | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')
# JSON doubles a Windows path's backslashes; forward slashes work there too.
target_dir=$(printf '%s' "$target_dir" | sed 's|\\\\|/|g')
out=${PREBUILT_OUT:-$target_dir/prebuilt}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
header=crates/freeqrent-ffi/include/freeqrent.h
mkdir -p "$out"

note() { echo "build-prebuilt: $*" >&2; }
skip() {
    if [ "$strict" = 1 ]; then
        note "error: $*"
        exit 1
    fi
    note "skipped: $*"
}

ndk_root() {
    for d in "${ANDROID_NDK_HOME:-}" "${ANDROID_NDK_LATEST_HOME:-}" "${ANDROID_NDK_ROOT:-}"; do
        if [ -n "$d" ] && [ -d "$d/toolchains/llvm/prebuilt" ]; then echo "$d"; return; fi
    done
    for base in "$HOME/Library/Android/sdk/ndk" "$HOME/Android/Sdk/ndk"; do
        if [ -d "$base" ]; then
            d=$(ls -d "$base"/*/ 2>/dev/null | sort -V | tail -1)
            if [ -n "$d" ] && [ -d "${d}toolchains/llvm/prebuilt" ]; then echo "${d%/}"; return; fi
        fi
    done
}

# Points cargo at the NDK's clang for an Android target; false without an NDK.
android_linker() {
    t=$1
    ndk=$(ndk_root)
    [ -n "$ndk" ] || return 1
    bin=$(ls -d "$ndk"/toolchains/llvm/prebuilt/*/bin | head -1)
    api=${ANDROID_API:-21}
    case "$t" in
        aarch64-linux-android) clang="aarch64-linux-android$api-clang" ;;
        armv7-linux-androideabi) clang="armv7a-linux-androideabi$api-clang" ;;
        x86_64-linux-android) clang="x86_64-linux-android$api-clang" ;;
        *) return 1 ;;
    esac
    [ -x "$bin/$clang" ] || return 1
    var=CARGO_TARGET_$(echo "$t" | tr 'a-z-' 'A-Z_')_LINKER
    export "$var=$bin/$clang"
    export "AR_$(echo "$t" | tr '-' '_')=$bin/llvm-ar"
}

# Static and dynamic library file names for a target.
names() {
    case "$1" in
        *-windows-msvc) static=freeqrent_ffi.lib; dynamic=freeqrent_ffi.dll ;;
        *-windows-gnu*) static=libfreeqrent_ffi.a; dynamic=freeqrent_ffi.dll ;;
        *-apple-*) static=libfreeqrent_ffi.a; dynamic=libfreeqrent_ffi.dylib ;;
        *) static=libfreeqrent_ffi.a; dynamic=libfreeqrent_ffi.so ;;
    esac
}

# Go's GOOS_GOARCH for the targets the Go module links.
goplatform() {
    case "$1" in
        x86_64-unknown-linux-gnu) echo linux_amd64 ;;
        aarch64-unknown-linux-gnu) echo linux_arm64 ;;
        x86_64-apple-darwin) echo darwin_amd64 ;;
        aarch64-apple-darwin) echo darwin_arm64 ;;
        x86_64-pc-windows-gnu) echo windows_amd64 ;;
        aarch64-pc-windows-gnullvm) echo windows_arm64 ;;
        *) echo "" ;;
    esac
}

build_native() {
    t=$1
    names "$t"
    dir="$out/$t"
    mkdir -p "$dir/lib" "$dir/include"
    cp "$header" "$dir/include/freeqrent.h"
    case "$t" in
        *-apple-darwin) export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0} ;;
        *-apple-ios*) export IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-13.0} ;;
    esac
    have_linker=1
    case "$t" in
        *-android*) android_linker "$t" || have_linker=0 ;;
    esac

    note "$t: static library"
    # --print native-static-libs only prints when rustc links, so the
    # staticlib is always rebuilt here (it's quick after the first build).
    touch crates/freeqrent-ffi/src/lib.rs
    log="$dir/rustc-static.log"
    if ! cargo rustc -q -p freeqrent-ffi --release --target "$t" --crate-type staticlib \
        -- --print native-static-libs >"$log" 2>&1; then
        cat "$log" >&2
        skip "$t: the static library didn't build"
        return 0
    fi
    sed -n 's/.*native-static-libs: //p' "$log" | tail -1 >"$dir/native-static-libs.txt"
    rm "$log"
    cp "$target_dir/$t/release/$static" "$dir/lib/$static"

    if [ "$have_linker" = 0 ]; then
        skip "$t: the dynamic library (no NDK linker found; set ANDROID_NDK_HOME)"
    else
        note "$t: dynamic library"
        if cargo rustc -q -p freeqrent-ffi --release --target "$t" --crate-type cdylib; then
            cp "$target_dir/$t/release/$dynamic" "$dir/lib/$dynamic"
            case "$t" in
                *-windows-msvc) cp "$target_dir/$t/release/freeqrent_ffi.dll.lib" "$dir/lib/freeqrent_ffi.dll.lib" ;;
                *-windows-gnu*) cp "$target_dir/$t/release/libfreeqrent_ffi.dll.a" "$dir/lib/libfreeqrent_ffi.dll.a" ;;
            esac
        else
            skip "$t: the dynamic library didn't link (no linker for this target?)"
        fi
    fi
}

build_wasm() {
    t=wasm32-unknown-unknown
    dir="$out/$t"
    mkdir -p "$dir/include"
    cp "$header" "$dir/include/freeqrent.h"
    note "$t: cdylib"
    if ! cargo build -q -p freeqrent-ffi --release --target "$t"; then
        skip "$t didn't build"
        return 0
    fi
    cp "$target_dir/$t/release/freeqrent_ffi.wasm" "$dir/freeqrent_ffi.wasm"
}

built=""
for t in $targets; do
    case "$t" in
        wasm32-unknown-unknown) build_wasm ;;
        *) build_native "$t" ;;
    esac
    built="$built $t"
done

# A universal macOS library, when both slices are there.
mac_a="$out/aarch64-apple-darwin/lib"
mac_x="$out/x86_64-apple-darwin/lib"
if command -v lipo >/dev/null 2>&1 && [ -f "$mac_a/libfreeqrent_ffi.a" ] && [ -f "$mac_x/libfreeqrent_ffi.a" ]; then
    note "universal-apple-darwin: lipo"
    u="$out/universal-apple-darwin"
    mkdir -p "$u/lib" "$u/include"
    cp "$header" "$u/include/freeqrent.h"
    cp "$out/aarch64-apple-darwin/native-static-libs.txt" "$u/native-static-libs.txt"
    lipo -create "$mac_a/libfreeqrent_ffi.a" "$mac_x/libfreeqrent_ffi.a" -output "$u/lib/libfreeqrent_ffi.a"
    if [ -f "$mac_a/libfreeqrent_ffi.dylib" ] && [ -f "$mac_x/libfreeqrent_ffi.dylib" ]; then
        lipo -create "$mac_a/libfreeqrent_ffi.dylib" "$mac_x/libfreeqrent_ffi.dylib" -output "$u/lib/libfreeqrent_ffi.dylib"
    fi
    built="$built universal-apple-darwin"
fi

# An iOS xcframework: the device slice, and the simulator slices lipo'd.
ios="$out/aarch64-apple-ios/lib/libfreeqrent_ffi.a"
sim_a="$out/aarch64-apple-ios-sim/lib/libfreeqrent_ffi.a"
sim_x="$out/x86_64-apple-ios/lib/libfreeqrent_ffi.a"
if command -v xcodebuild >/dev/null 2>&1 && [ -f "$ios" ] && { [ -f "$sim_a" ] || [ -f "$sim_x" ]; }; then
    note "freeqrent-ffi.xcframework"
    sim="$out/ios-simulator/lib"
    mkdir -p "$sim"
    if [ -f "$sim_a" ] && [ -f "$sim_x" ]; then
        lipo -create "$sim_a" "$sim_x" -output "$sim/libfreeqrent_ffi.a"
    elif [ -f "$sim_a" ]; then
        cp "$sim_a" "$sim/libfreeqrent_ffi.a"
    else
        cp "$sim_x" "$sim/libfreeqrent_ffi.a"
    fi
    hdrs="$out/xcframework-headers"
    mkdir -p "$hdrs"
    cp "$header" "$hdrs/freeqrent.h"
    xc="$out/freeqrent-ffi.xcframework"
    if [ -d "$xc" ]; then rm -r "$xc"; fi
    xcodebuild -create-xcframework \
        -library "$ios" -headers "$hdrs" \
        -library "$sim/libfreeqrent_ffi.a" -headers "$hdrs" \
        -output "$xc" >/dev/null
    built="$built xcframework"
fi

# The Go module's copies, and a check that its link flags are complete.
if [ "$go" = 1 ]; then
    for t in $built; do
        p=$(goplatform "$t")
        if [ -n "$p" ] && [ -f "$out/$t/lib/libfreeqrent_ffi.a" ]; then
            mkdir -p "bindings/go/lib/$p"
            cp "$out/$t/lib/libfreeqrent_ffi.a" "bindings/go/lib/$p/libfreeqrent_ffi.a"
            goos=${p%_*}
            flags="bindings/go/link_$goos.go"
            for lib in $(cat "$out/$t/native-static-libs.txt"); do
                case "$lib" in
                    # Go links libSystem on macOS itself, and warns when it is given twice.
                    -framework|-lSystem) continue ;;
                esac
                if ! grep -qwF -- "$lib" "$flags"; then
                    note "error: $t needs $lib, which $flags doesn't link (rustc: $(cat "$out/$t/native-static-libs.txt"))"
                    exit 1
                fi
            done
            note "go: lib/$p/libfreeqrent_ffi.a"
        elif [ "$t" = wasm32-unknown-unknown ] && [ -f "$out/$t/freeqrent_ffi.wasm" ]; then
            mkdir -p bindings/go/wasm
            cp "$out/$t/freeqrent_ffi.wasm" bindings/go/wasm/freeqrent_ffi.wasm
            note "go: wasm/freeqrent_ffi.wasm"
        fi
    done
fi

sha() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}

if [ "$package" = 1 ]; then
    dist="$out/dist"
    mkdir -p "$dist"
    sums="$dist/SHA256SUMS"
    : >"$sums"
    for t in $built; do
        case "$t" in
            xcframework) src=freeqrent-ffi.xcframework ;;
            *) src=$t ;;
        esac
        [ -e "$out/$src" ] || continue
        name="freeqrent-ffi-$version-$t"
        case "$t" in
            *-windows-*)
                file="$name.zip"
                if [ -f "$dist/$file" ]; then rm "$dist/$file"; fi
                if command -v zip >/dev/null 2>&1; then
                    (cd "$out" && zip -qr "dist/$file" "$src")
                else
                    (cd "$out" && 7z a -tzip "dist/$file" "$src" >/dev/null)
                fi ;;
            *)
                file="$name.tar.gz"
                tar -czf "$dist/$file" -C "$out" "$src" ;;
        esac
        (cd "$dist" && sha "$file" >"$file.sha256" && cat "$file.sha256" >>SHA256SUMS)
    done
fi

# Sizes, for the log.
for t in $built; do
    case "$t" in
        xcframework) f="$out/freeqrent-ffi.xcframework" ;;
        wasm32-unknown-unknown) f="$out/$t/freeqrent_ffi.wasm" ;;
        *) f="$out/$t/lib" ;;
    esac
    [ -e "$f" ] || continue
    if [ "$t" = xcframework ]; then
        printf '%10s  %s\n' "$(du -sk "$f" | cut -f1)K" "freeqrent-ffi.xcframework"
    elif [ -d "$f" ]; then
        for g in "$f"/*; do
            if [ -f "$g" ]; then printf '%10d  %s\n' "$(wc -c <"$g" | tr -d ' ')" "${g#"$out"/}"; fi
        done
    else
        printf '%10d  %s\n' "$(wc -c <"$f" | tr -d ' ')" "${f#"$out"/}"
    fi
done
