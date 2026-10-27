//go:build cgo && !freeqrent_wasm

package freeqrent

/*
#cgo CFLAGS: -I${SRCDIR}/include
#include <stdlib.h>
#include "freeqrent.h"
*/
import "C"

import "unsafe"

const backendName = "cgo"

// The C ABI is pure and safe from any thread, so one value serves every
// goroutine.
type cgoBackend struct{}

func getBackend() (backend, error) { return cgoBackend{}, nil }

func cbytes(b []byte) *C.uint8_t {
	if len(b) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0]))
}

// ctext points at NUL-terminated text (see cText).
func ctext(b []byte) *C.char { return (*C.char)(unsafe.Pointer(&b[0])) }

// take copies the library's string into Go and frees it.
func take(out *C.char, n C.size_t, status C.int32_t) (string, int32, error) {
	if out == nil {
		return "", int32(status), nil
	}
	s := C.GoStringN(out, C.int(n))
	C.freeqrent_free(out)
	return s, int32(status), nil
}

func (cgoBackend) payload(fn payloadFn, data []byte, kind int32, style []byte, ms uint32) (string, int32, error) {
	var out *C.char
	var n C.size_t
	var st C.int32_t
	p, l, k, s := cbytes(data), C.size_t(len(data)), C.int32_t(kind), ctext(style)
	switch fn {
	case fnRender:
		st = C.freeqrent_render(p, l, k, s, &out, &n)
	case fnRenderFrame:
		st = C.freeqrent_render_frame(p, l, k, s, C.uint32_t(ms), &out, &n)
	case fnCheck:
		st = C.freeqrent_check(p, l, k, s, &out, &n)
	case fnPlan:
		st = C.freeqrent_plan(p, l, k, s, &out, &n)
	}
	return take(out, n, st)
}

func (cgoBackend) image(rgba []byte, width, height uint32, opts []byte) (string, int32, error) {
	var out *C.char
	var n C.size_t
	st := C.freeqrent_palette_from_image(cbytes(rgba), C.size_t(len(rgba)), C.uint32_t(width), C.uint32_t(height), ctext(opts), &out, &n)
	return take(out, n, st)
}

func (cgoBackend) text(fn textFn, text, opts []byte) (string, int32, error) {
	var out *C.char
	var n C.size_t
	var st C.int32_t
	switch fn {
	case fnPaletteCSS:
		st = C.freeqrent_palette_from_css(ctext(text), ctext(opts), &out, &n)
	case fnPaletteHTML:
		st = C.freeqrent_palette_from_html(ctext(text), ctext(opts), &out, &n)
	}
	return take(out, n, st)
}

func (cgoBackend) presets() (string, int32, error) {
	var out *C.char
	var n C.size_t
	st := C.freeqrent_presets(&out, &n)
	return take(out, n, st)
}

func (cgoBackend) version() string { return C.GoString(C.freeqrent_version()) }
