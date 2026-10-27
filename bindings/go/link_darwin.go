//go:build darwin && cgo && !freeqrent_wasm

package freeqrent

// The prebuilt static library and what rustc says it needs on macOS, less
// -lSystem, which Go links itself (cargo rustc --print native-static-libs;
// scripts/build-prebuilt.sh --go checks the list).

// #cgo arm64 LDFLAGS: -L${SRCDIR}/lib/darwin_arm64
// #cgo amd64 LDFLAGS: -L${SRCDIR}/lib/darwin_amd64
// #cgo LDFLAGS: -lfreeqrent_ffi -lc -lm
import "C"
