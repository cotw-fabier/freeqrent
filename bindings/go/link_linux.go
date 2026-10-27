//go:build linux && cgo && !freeqrent_wasm

package freeqrent

// The prebuilt static library and what rustc says it needs on Linux (gnu)
// (cargo rustc --print native-static-libs; scripts/build-prebuilt.sh --go
// checks the list).

// #cgo amd64 LDFLAGS: -L${SRCDIR}/lib/linux_amd64
// #cgo arm64 LDFLAGS: -L${SRCDIR}/lib/linux_arm64
// #cgo LDFLAGS: -lfreeqrent_ffi -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
import "C"
