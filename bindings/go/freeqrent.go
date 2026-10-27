package freeqrent

import (
	"bytes"
	"encoding/json"
	"fmt"
	"image"
	"image/draw"
	"math"
	"sync"
	"time"
)

// Payload kinds, as in freeqrent.h.
const (
	kindText  int32 = 0
	kindBytes int32 = 1
)

// The functions that take a payload and a style.
type payloadFn int

const (
	fnRender payloadFn = iota
	fnRenderFrame
	fnCheck
	fnPlan
)

// The functions that take a text and palette options.
type textFn int

const (
	fnPaletteCSS textFn = iota
	fnPaletteHTML
)

// Status codes, as in freeqrent.h.
const (
	statusOK      int32 = 0
	statusError   int32 = 1
	statusInvalid int32 = 2
)

// Render returns the SVG for text (UTF-8, segmented into the densest QR
// modes) drawn in style. A nil style is the default style.
func Render(text string, style StyleDoc) (string, error) {
	return callPayload(fnRender, []byte(text), kindText, style, 0)
}

// RenderBytes is Render for a byte payload, encoded in byte mode.
func RenderBytes(data []byte, style StyleDoc) (string, error) {
	return callPayload(fnRender, data, kindBytes, style, 0)
}

// RenderFrame returns the style's intro stopped at t (whole milliseconds,
// 0 to about 49 days), as a still SVG with no animation. A style without an
// intro gives its finished code.
func RenderFrame(text string, style StyleDoc, t time.Duration) (string, error) {
	ms, err := millis(t)
	if err != nil {
		return "", err
	}
	return callPayload(fnRenderFrame, []byte(text), kindText, style, ms)
}

// RenderFrameBytes is RenderFrame for a byte payload.
func RenderFrameBytes(data []byte, style StyleDoc, t time.Duration) (string, error) {
	ms, err := millis(t)
	if err != nil {
		return "", err
	}
	return callPayload(fnRenderFrame, data, kindBytes, style, ms)
}

// Check returns the scan report for text drawn in style. A style Render
// would refuse still gets a report, with OK false and the reasons in
// Errors; only a bad document or a payload that doesn't fit is an error.
func Check(text string, style StyleDoc) (Report, error) {
	return report(callPayload(fnCheck, []byte(text), kindText, style, 0))
}

// CheckBytes is Check for a byte payload.
func CheckBytes(data []byte, style StyleDoc) (Report, error) {
	return report(callPayload(fnCheck, data, kindBytes, style, 0))
}

// Plan returns the style's animation plan (SPEC §8.5): the static geometry
// and each group's keyframes, for players outside CSS. The plan isn't in
// the schema, so it comes back as JSON for json.Unmarshal.
func Plan(text string, style StyleDoc) (json.RawMessage, error) {
	out, err := callPayload(fnPlan, []byte(text), kindText, style, 0)
	return json.RawMessage(out), err
}

// PlanBytes is Plan for a byte payload.
func PlanBytes(data []byte, style StyleDoc) (json.RawMessage, error) {
	out, err := callPayload(fnPlan, data, kindBytes, style, 0)
	return json.RawMessage(out), err
}

// PaletteFromImage extracts a palette from width×height pixels of 8-bit
// RGBA (not premultiplied), row by row. The result's Palette drops into a
// style with WithPalette.
func PaletteFromImage(rgba []byte, width, height int, opts PaletteOpts) (PaletteResult, error) {
	if width < 0 || height < 0 || uint64(width) > math.MaxUint32 || uint64(height) > math.MaxUint32 {
		return PaletteResult{}, argumentError(fmt.Sprintf("the image size %d×%d is out of range", width, height))
	}
	o, err := json.Marshal(opts)
	if err != nil {
		return PaletteResult{}, err
	}
	return palette(paletteImageRaw(rgba, uint32(width), uint32(height), o))
}

func paletteImageRaw(rgba []byte, width, height uint32, opts []byte) (string, error) {
	o, err := cText(opts, "the options")
	if err != nil {
		return "", err
	}
	return withBackend(func(b backend) (string, int32, error) { return b.image(rgba, width, height, o) })
}

// PaletteFromImageOf is PaletteFromImage for an image.Image.
func PaletteFromImageOf(img image.Image, opts PaletteOpts) (PaletteResult, error) {
	im := ImageOf(img)
	return PaletteFromImage(im.RGBA, int(im.Width), int(im.Height), opts)
}

// PaletteFromCSS extracts a palette from a style sheet's colors.
func PaletteFromCSS(css string, opts PaletteOpts) (PaletteResult, error) {
	return paletteText(fnPaletteCSS, css, opts)
}

// PaletteFromHTML extracts a palette from a page's colors: its style
// sheets and inline styles. Nothing is fetched.
func PaletteFromHTML(html string, opts PaletteOpts) (PaletteResult, error) {
	return paletteText(fnPaletteHTML, html, opts)
}

// ImageOf converts an image.Image to the Style document's Image (8-bit
// RGBA, not premultiplied), for a halftone, an art-mode picture or
// PaletteFromImage.
func ImageOf(img image.Image) Image {
	b := img.Bounds()
	n, ok := img.(*image.NRGBA)
	if !ok || n.Stride != 4*b.Dx() || b.Min != (image.Point{}) {
		n = image.NewNRGBA(image.Rect(0, 0, b.Dx(), b.Dy()))
		draw.Draw(n, n.Bounds(), img, b.Min, draw.Src)
	}
	return Image{Width: uint32(b.Dx()), Height: uint32(b.Dy()), RGBA: n.Pix[:4*b.Dx()*b.Dy()]}
}

func presetsRaw() (string, error) {
	return withBackend(func(b backend) (string, int32, error) { return b.presets() })
}

// Presets returns the preset names, the values WithPreset takes.
func Presets() ([]Preset, error) {
	out, err := presetsRaw()
	if err != nil {
		return nil, err
	}
	var names []Preset
	if err := json.Unmarshal([]byte(out), &names); err != nil {
		return nil, internalError("the preset list isn't a JSON array: " + err.Error())
	}
	return names, nil
}

var version = sync.OnceValue(func() string {
	b, err := getBackend()
	if err != nil {
		return ""
	}
	return b.version()
})

// Version returns the library's version, e.g. "1.0.0", the same in every
// freeQRent package (SPEC §10.3). It is "" if the WebAssembly build can't
// start.
func Version() string { return version() }

// Backend says which build is linked: "cgo" or "wasm".
func Backend() string { return backendName }

func millis(t time.Duration) (uint32, error) {
	if t < 0 || t.Milliseconds() > math.MaxUint32 {
		return 0, argumentError(fmt.Sprintf("the frame time %v is out of range", t))
	}
	return uint32(t.Milliseconds()), nil
}

// styleText is the document as NUL-terminated text for the C ABI.
func styleText(style StyleDoc) ([]byte, error) {
	if style == nil {
		return []byte("{}\x00"), nil
	}
	data, err := style.StyleJSON()
	if err != nil {
		return nil, fmt.Errorf("freeqrent: writing the style document: %w", err)
	}
	return cText(data, "the style document")
}

func cText(data []byte, what string) ([]byte, error) {
	if bytes.IndexByte(data, 0) >= 0 {
		return nil, argumentError(what + " holds a NUL byte")
	}
	out := make([]byte, len(data)+1)
	copy(out, data)
	return out, nil
}

func callPayload(fn payloadFn, data []byte, kind int32, style StyleDoc, ms uint32) (string, error) {
	s, err := styleText(style)
	if err != nil {
		return "", err
	}
	return withBackend(func(b backend) (string, int32, error) { return b.payload(fn, data, kind, s, ms) })
}

func paletteText(fn textFn, text string, opts PaletteOpts) (PaletteResult, error) {
	o, err := json.Marshal(opts)
	if err != nil {
		return PaletteResult{}, err
	}
	return palette(paletteTextRaw(fn, text, o))
}

func paletteTextRaw(fn textFn, text string, opts []byte) (string, error) {
	t, err := cText([]byte(text), "the text")
	if err != nil {
		return "", err
	}
	o, err := cText(opts, "the options")
	if err != nil {
		return "", err
	}
	return withBackend(func(b backend) (string, int32, error) { return b.text(fn, t, o) })
}

func withBackend(f func(backend) (string, int32, error)) (string, error) {
	b, err := getBackend()
	if err != nil {
		return "", err
	}
	out, status, err := f(b)
	if err != nil {
		return "", err
	}
	switch status {
	case statusOK:
		return out, nil
	case statusError:
		return "", parseError(out)
	}
	return "", internalError(fmt.Sprintf("the library returned status %d", status))
}

func report(out string, err error) (Report, error) {
	if err != nil {
		return Report{}, err
	}
	var r Report
	if err := json.Unmarshal([]byte(out), &r); err != nil {
		return Report{}, internalError("the report doesn't parse: " + err.Error())
	}
	return r, nil
}

func palette(out string, err error) (PaletteResult, error) {
	if err != nil {
		return PaletteResult{}, err
	}
	var r PaletteResult
	if err := json.Unmarshal([]byte(out), &r); err != nil {
		return PaletteResult{}, internalError("the palette doesn't parse: " + err.Error())
	}
	return r, nil
}

// backend is one build of the library: cgo (backend_cgo.go) or
// WebAssembly on wazero (backend_wazero.go). Text arguments are NUL-terminated.
type backend interface {
	payload(fn payloadFn, data []byte, kind int32, style []byte, ms uint32) (string, int32, error)
	image(rgba []byte, width, height uint32, opts []byte) (string, int32, error)
	text(fn textFn, text, opts []byte) (string, int32, error)
	presets() (string, int32, error)
	version() string
}
