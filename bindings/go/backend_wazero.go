//go:build freeqrent_wasm

package freeqrent

import (
	"context"
	_ "embed"
	"fmt"
	"runtime"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

const backendName = "wasm"

// freeqrent-ffi built for wasm32-unknown-unknown. It isn't committed: build
// it with
//
//	scripts/build-prebuilt.sh --go wasm32-unknown-unknown
//
// from the repository root. Without it, go build stops here with "pattern
// wasm/freeqrent_ffi.wasm: no matching files found".
//
//go:embed wasm/freeqrent_ffi.wasm
var wasmModule []byte

// The module is compiled once. Each instance has its own linear memory and
// runs one call at a time, so calls go through a pool of instances, at most
// GOMAXPROCS of them: goroutines render in parallel up to that, and wait
// beyond it. (A single instance behind a mutex would serialize every call;
// a sync.Pool would drop instances without closing them.) An instance whose
// call trapped is closed rather than reused.
type wasmBackend struct {
	rt       wazero.Runtime
	compiled wazero.CompiledModule
	slots    chan struct{}
	idle     chan *instance
	ver      string
}

type instance struct {
	mod                                                   api.Module
	mem                                                   api.Memory
	alloc, dealloc, free                                  api.Function
	render, renderFrame, check, plan                      api.Function
	paletteImage, paletteCSS, paletteHTML, presetsFn, ver api.Function
}

var ctx = context.Background()

// runtimeConfig is wazero's compiler, except on amd64: there, wazero
// 1.12's compiler gets one conformance case wrong (a palette's dark twin
// comes out with different lightness), while its interpreter matches the
// library byte for byte, so amd64 runs interpreted until that's fixed
// upstream. The arm64 compiler passes the whole suite.
func runtimeConfig() wazero.RuntimeConfig {
	if runtime.GOARCH == "amd64" {
		return wazero.NewRuntimeConfigInterpreter()
	}
	return wazero.NewRuntimeConfig()
}

var loadBackend = sync.OnceValues(func() (*wasmBackend, error) {
	rt := wazero.NewRuntimeWithConfig(ctx, runtimeConfig())
	compiled, err := rt.CompileModule(ctx, wasmModule)
	if err != nil {
		rt.Close(ctx)
		return nil, internalError("compiling the WebAssembly module: " + err.Error())
	}
	n := runtime.GOMAXPROCS(0)
	b := &wasmBackend{rt: rt, compiled: compiled, slots: make(chan struct{}, n), idle: make(chan *instance, n)}
	in, err := b.instantiate()
	if err != nil {
		return nil, err
	}
	b.ver, err = in.version()
	if err != nil {
		return nil, err
	}
	b.idle <- in
	return b, nil
})

func getBackend() (backend, error) {
	b, err := loadBackend()
	if err != nil {
		return nil, err
	}
	return b, nil
}

func (b *wasmBackend) instantiate() (*instance, error) {
	mod, err := b.rt.InstantiateModule(ctx, b.compiled, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return nil, internalError("starting the WebAssembly module: " + err.Error())
	}
	in := &instance{mod: mod, mem: mod.Memory()}
	for name, f := range map[string]*api.Function{
		"freeqrent_alloc":              &in.alloc,
		"freeqrent_dealloc":            &in.dealloc,
		"freeqrent_free":               &in.free,
		"freeqrent_render":             &in.render,
		"freeqrent_render_frame":       &in.renderFrame,
		"freeqrent_check":              &in.check,
		"freeqrent_plan":               &in.plan,
		"freeqrent_palette_from_image": &in.paletteImage,
		"freeqrent_palette_from_css":   &in.paletteCSS,
		"freeqrent_palette_from_html":  &in.paletteHTML,
		"freeqrent_presets":            &in.presetsFn,
		"freeqrent_version":            &in.ver,
	} {
		if *f = mod.ExportedFunction(name); *f == nil {
			mod.Close(ctx)
			return nil, internalError("the WebAssembly module doesn't export " + name)
		}
	}
	if in.mem == nil {
		mod.Close(ctx)
		return nil, internalError("the WebAssembly module has no memory")
	}
	return in, nil
}

// with runs f on an instance from the pool.
func (b *wasmBackend) with(f func(*instance) (string, int32, error)) (string, int32, error) {
	b.slots <- struct{}{}
	defer func() { <-b.slots }()
	var in *instance
	select {
	case in = <-b.idle:
	default:
		var err error
		if in, err = b.instantiate(); err != nil {
			return "", 0, err
		}
	}
	out, status, err := f(in)
	if err != nil {
		in.mod.Close(ctx)
		return "", 0, err
	}
	b.idle <- in
	return out, status, nil
}

// A failed call into an instance (a trap, or memory out of range).
func trap(err error) error {
	return internalError("the WebAssembly call failed: " + err.Error())
}

func (in *instance) call(f api.Function, args ...uint64) (uint64, error) {
	r, err := f.Call(ctx, args...)
	if err != nil {
		return 0, trap(err)
	}
	if len(r) == 0 {
		return 0, nil
	}
	return r[0], nil
}

// put copies b into a fresh allocation of the instance's memory; the
// pointer is 0 for empty b.
func (in *instance) put(b []byte) (uint32, error) {
	if len(b) == 0 {
		return 0, nil
	}
	p, err := in.call(in.alloc, uint64(len(b)))
	if err != nil {
		return 0, err
	}
	ptr := api.DecodeU32(p)
	if ptr == 0 || !in.mem.Write(ptr, b) {
		return 0, trap(fmt.Errorf("couldn't place %d bytes", len(b)))
	}
	return ptr, nil
}

// args places each argument in memory and runs f with their pointers and
// an 8-byte out cell (the char* then the size_t, both 32-bit here), then
// reads the result string and frees everything.
func (in *instance) args(f func(ptrs []uint32, out uint32) (uint64, error), bufs ...[]byte) (string, int32, error) {
	ptrs := make([]uint32, len(bufs))
	defer func() {
		for i, p := range ptrs {
			if p != 0 {
				in.call(in.dealloc, uint64(p), uint64(len(bufs[i])))
			}
		}
	}()
	for i, b := range bufs {
		p, err := in.put(b)
		if err != nil {
			return "", 0, err
		}
		ptrs[i] = p
	}
	cell, err := in.put(make([]byte, 8))
	if err != nil {
		return "", 0, err
	}
	defer in.call(in.dealloc, uint64(cell), 8)
	status, err := f(ptrs, cell)
	if err != nil {
		return "", 0, err
	}
	sp, ok1 := in.mem.ReadUint32Le(cell)
	sn, ok2 := in.mem.ReadUint32Le(cell + 4)
	if !ok1 || !ok2 {
		return "", 0, trap(fmt.Errorf("the out cell is out of range"))
	}
	if sp == 0 {
		return "", int32(api.DecodeI32(status)), nil
	}
	raw, ok := in.mem.Read(sp, sn)
	if !ok {
		return "", 0, trap(fmt.Errorf("the result is out of range"))
	}
	s := string(raw)
	if _, err := in.call(in.free, uint64(sp)); err != nil {
		return "", 0, err
	}
	return s, int32(api.DecodeI32(status)), nil
}

func (b *wasmBackend) payload(fn payloadFn, data []byte, kind int32, style []byte, ms uint32) (string, int32, error) {
	return b.with(func(in *instance) (string, int32, error) {
		return in.args(func(p []uint32, out uint32) (uint64, error) {
			d, n, k, s := uint64(p[0]), uint64(len(data)), api.EncodeI32(kind), uint64(p[1])
			switch fn {
			case fnRenderFrame:
				return in.call(in.renderFrame, d, n, k, s, api.EncodeU32(ms), uint64(out), uint64(out+4))
			case fnCheck:
				return in.call(in.check, d, n, k, s, uint64(out), uint64(out+4))
			case fnPlan:
				return in.call(in.plan, d, n, k, s, uint64(out), uint64(out+4))
			}
			return in.call(in.render, d, n, k, s, uint64(out), uint64(out+4))
		}, data, style)
	})
}

func (b *wasmBackend) image(rgba []byte, width, height uint32, opts []byte) (string, int32, error) {
	return b.with(func(in *instance) (string, int32, error) {
		return in.args(func(p []uint32, out uint32) (uint64, error) {
			return in.call(in.paletteImage, uint64(p[0]), uint64(len(rgba)), api.EncodeU32(width), api.EncodeU32(height), uint64(p[1]), uint64(out), uint64(out+4))
		}, rgba, opts)
	})
}

func (b *wasmBackend) text(fn textFn, text, opts []byte) (string, int32, error) {
	return b.with(func(in *instance) (string, int32, error) {
		return in.args(func(p []uint32, out uint32) (uint64, error) {
			f := in.paletteCSS
			if fn == fnPaletteHTML {
				f = in.paletteHTML
			}
			return in.call(f, uint64(p[0]), uint64(p[1]), uint64(out), uint64(out+4))
		}, text, opts)
	})
}

func (b *wasmBackend) presets() (string, int32, error) {
	return b.with(func(in *instance) (string, int32, error) {
		return in.args(func(_ []uint32, out uint32) (uint64, error) {
			return in.call(in.presetsFn, uint64(out), uint64(out+4))
		})
	})
}

func (b *wasmBackend) version() string { return b.ver }

func (in *instance) version() (string, error) {
	p, err := in.call(in.ver)
	if err != nil {
		return "", err
	}
	ptr := api.DecodeU32(p)
	for n := uint32(0); n < 64; n++ {
		c, ok := in.mem.ReadByte(ptr + n)
		if !ok {
			break
		}
		if c == 0 {
			raw, _ := in.mem.Read(ptr, n)
			return string(raw), nil
		}
	}
	return "", internalError("the WebAssembly module's version isn't a short C string")
}
