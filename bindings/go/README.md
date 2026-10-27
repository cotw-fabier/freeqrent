# freeqrent-go

Styled and animated SVG QR codes for Go, from freeQRent's Rust core. The output is deterministic: the same payload and style give the same bytes as the Rust crate and every other freeQRent binding (the shared conformance suite checks this).

```go
import "github.com/freeqrent/freeqrent-go"

svg, err := freeqrent.Render("https://example.com", freeqrent.NewStyle(
	freeqrent.WithPreset(freeqrent.PresetCandy),
	freeqrent.WithModules(freeqrent.Dots{Size: freeqrent.Ptr(0.8)}),
	freeqrent.WithIntro(freeqrent.IntroRipple),
))
```

## Install

```sh
go get github.com/freeqrent/freeqrent-go
```

There are two builds, and both produce the same bytes:

| Build | How | Needs | Speed |
|---|---|---|---|
| **cgo** (default) | links the prebuilt static library `lib/<goos>_<goarch>/libfreeqrent_ffi.a` (linux, darwin, windows × amd64, arm64) | a C toolchain (on Windows, MinGW gcc); no Rust | native |
| **wasm** | `-tags freeqrent_wasm`: runs an embedded WebAssembly build of the same library on [wazero](https://github.com/tetratelabs/wazero), in pure Go | nothing; `CGO_ENABLED=0` works | about 2.6× slower, plus about 150 ms to compile the module on first use |

```sh
CGO_ENABLED=0 go build -tags freeqrent_wasm ./...
```

Building without cgo and without the tag fails with `undefined: freeqrent_needs_cgo_or_the_freeqrent_wasm_build_tag`.

Measured on an Apple M4 Max (darwin/arm64, Go 1.27), rendering a version 10 code in the ink preset (`BenchmarkRenderV10`): cgo 0.43 ms, wasm 1.12 ms (2.6×). Across 14 goroutines (`BenchmarkRenderV10Parallel`): cgo 58 µs, wasm 122 µs per render (2.1×). The darwin/arm64 static library is 19.5 MB (most of it is dropped at link time: a program that renders one code is 8.1 MB with cgo), and the `.wasm` is 835 KB (a wasm-build program is 9.4 MB).

## API

Every function is safe for concurrent use.

| Function | Returns |
|---|---|
| `Render(text, style)`, `RenderBytes(data, style)` | the SVG. Text is UTF-8, segmented into the densest QR modes; bytes go in byte mode |
| `RenderFrame(text, style, t)`, `RenderFrameBytes` | the intro stopped at `t` (a `time.Duration`, whole milliseconds), as a still SVG |
| `Check(text, style)`, `CheckBytes` | a typed `Report`: `OK`, `Errors`, `Warnings`, `MinContrast`, `ECCBudgetUsed`, `LogoScale`, `Steering`, `Code` (version, ECC, size, mask). A style `Render` would refuse still gets a report; `Report.Err()` gives its first error |
| `Plan(text, style)`, `PlanBytes` | the animation plan (SPEC §8.5) as `json.RawMessage`; the plan isn't in the schema, so there's no typed form |
| `PaletteFromImage(rgba, w, h, opts)` | a `PaletteResult` from 8-bit RGBA pixels (not premultiplied) |
| `PaletteFromImageOf(img, opts)` | the same for an `image.Image` (any color model) |
| `PaletteFromCSS(css, opts)`, `PaletteFromHTML(html, opts)` | a `PaletteResult` from a style sheet's or a page's colors; nothing is fetched |
| `ImageOf(img)` | an `image.Image` as the document's `Image`, for a halftone or art-mode picture |
| `Presets()` | the preset names, as `[]Preset` |
| `Version()` | `"1.0.0"`, the same in every freeQRent package |
| `Backend()` | `"cgo"` or `"wasm"` |

A `PaletteResult`'s `Palette` drops straight into a style: `freeqrent.WithPalette(res.Palette)`.

### Styles

A style is any `StyleDoc`. Three ways to make one:

```go
// Functional options: one With… per key of the Style document, plus WithToken.
s := freeqrent.NewStyle(
	freeqrent.WithPreset(freeqrent.PresetInk),
	freeqrent.WithToken(freeqrent.TokenFG, "#1d3b72"),
	freeqrent.WithFrame(freeqrent.FrameCircle{Ring: freeqrent.FrameRingDashed}),
)
s = s.With(freeqrent.WithDark(true)) // a copy with more options

// A struct literal.
s = freeqrent.Style{
	Preset:  freeqrent.PresetInk,
	Finders: &freeqrent.Finders{Ring: freeqrent.RingSquircle{}, Eye: freeqrent.EyeCircle{}},
	Intro:   freeqrent.IntroOptions{Preset: freeqrent.IntroSweep, DurationMS: freeqrent.Ptr[uint32](900)},
}

// A raw document: JSON text, or anything encoding/json marshals (a map[string]any).
d := freeqrent.Document(`{"preset":"ink","modules":{"kind":"dots","size":0.9}}`)
d = freeqrent.Document(map[string]any{"preset": "ink"})
```

A nil `StyleDoc` is the default style.

The types are generated from `schema/freeqrent.schema.json`:

- Each object is a struct. Unset fields are left out of the document, so the core's defaults (or the preset's) apply. Optional numbers, booleans and strings are pointers, so zero can still be said: use `freeqrent.Ptr(0.8)`, or `new(0.8)` from Go 1.26.
- Each enum is a string type with constants (`PresetInk`, `TokenFG`, `ECCQ`, `IntroRipple`, …).
- Each `kind` union is a sealed interface with one struct per variant, which marshals with its `kind`: `ModuleShape` is `Square`, `Rounded`, `Dots`, `Liquid` or `Bars`; `Ring` is `RingSquare`, `RingRounded`, …; then `Eye…`, `Frame…`, `Border…`, `Effect…`, `Texture…`, `Ease…`, `Stagger…`, `Trigger…`, `TargetSize…`, `LogoFrame…`, `LogoShape…`, `Ink…` and `AlignmentShape…`.
- The other unions are interfaces too: an `Intro` is an `IntroPreset` name, `IntroOptions` or a `PaletteShift`; a `Caption` is `CaptionText` or `CaptionOptions`; a `Clarity` is a `ClarityLevel` or an `Image`.

Every struct also reads strictly with `json.Unmarshal`, so a document parses into a `Style` and unknown keys are errors. The result types (`Report`, `PaletteResult`, `Error`) read leniently, skipping keys a later version may add.

### Errors

Errors from the library are `*freeqrent.Error`, with a stable `Code`, an English `Message` and the fields in `Detail` (a `map[string]any`; numbers are `float64`). The codes are the ones in SPEC §10.1: `invalid_style` (`path`, `reason`, and `offset` for text that isn't JSON), `invalid_logo`, `invalid_image`, `invalid_argument`, `data_too_long`, `contrast_too_low`, `option_out_of_range`, `logo_too_large`, `no_colors` and the rest.

```go
var e *freeqrent.Error
if errors.As(err, &e) && e.Code == "contrast_too_low" {
	fmt.Println(e.Detail["token"], e.Detail["delta_l"])
}
errors.Is(err, &freeqrent.Error{Code: "data_too_long"}) // matches on the code
```

A NUL byte in a document or palette text, a negative frame time, or an image size out of range is an `invalid_argument` raised on the Go side. A `Document` that encoding/json can't marshal returns encoding/json's error, wrapped.

## Build from source

The static libraries and the `.wasm` aren't committed. From the repository root (needs Rust):

```sh
scripts/build-prebuilt.sh --go aarch64-apple-darwin wasm32-unknown-unknown
cd bindings/go
go test ./...                                         # cgo
CGO_ENABLED=0 go test -tags freeqrent_wasm ./...      # wasm
```

`--go` copies each static library to `lib/<goos>_<goarch>/` and the `.wasm` to `wasm/`, and checks that the link flags in `link_<goos>.go` cover what `cargo rustc --print native-static-libs` says the library needs. With no targets the script builds the host and every installed rustup target it knows. Windows uses the `x86_64-pc-windows-gnu` (or `aarch64-pc-windows-gnullvm`) library, since cgo links with MinGW.

`include/freeqrent.h` is a copy of `crates/freeqrent-ffi/include/freeqrent.h`; a test fails when they differ.

The tests run the shared conformance suite (`conformance/cases.json`, 77 cases) three ways: re-serialized with `encoding/json` (which escapes `<`, `>` and `&`), without the escaping, and read into the generated types and marshalled back. They also check typed styles against their JSON, every error family, byte payloads, palettes, and goroutines rendering at once.

## Regenerate the typed styles

```sh
cd bindings/go
go generate
```

This runs `go run ./internal/gen/cmd -schema ../../schema/freeqrent.schema.json -out style_gen.go`. A test fails when `style_gen.go` is stale.
