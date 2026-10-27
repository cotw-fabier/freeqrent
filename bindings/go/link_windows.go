//go:build windows && cgo && !freeqrent_wasm

package freeqrent

// The prebuilt static library and what rustc says it needs on Windows.
// cgo links with MinGW, so the library is built for the windows-gnu
// targets (x86_64-pc-windows-gnu, aarch64-pc-windows-gnullvm), not msvc
// (cargo rustc --print native-static-libs; scripts/build-prebuilt.sh --go
// checks the list).

// #cgo amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64
// #cgo arm64 LDFLAGS: -L${SRCDIR}/lib/windows_arm64
// #cgo LDFLAGS: -lfreeqrent_ffi -lkernel32 -ladvapi32 -lntdll -luserenv -lws2_32 -ldbghelp -lbcrypt -lsynchronization -lgcc_eh -l:libpthread.a
import "C"
