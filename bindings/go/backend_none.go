//go:build !cgo && !freeqrent_wasm

package freeqrent

// Without cgo the default build has no library to call. Build with cgo
// (CGO_ENABLED=1 and a C compiler), or use the pure-Go WebAssembly build:
//
//	CGO_ENABLED=0 go build -tags freeqrent_wasm
var _ = freeqrent_needs_cgo_or_the_freeqrent_wasm_build_tag
