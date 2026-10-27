// Package freeqrent draws styled and animated QR codes as SVG, with
// freeQRent's Rust core behind it (SPEC §10.2). Output is deterministic:
// the same payload and style give byte-identical SVG in every language
// freeQRent ships in.
//
//	svg, err := freeqrent.Render("https://example.com", freeqrent.NewStyle(
//		freeqrent.WithPreset(freeqrent.PresetCandy),
//		freeqrent.WithModules(freeqrent.Dots{Size: freeqrent.Ptr(0.8)}),
//		freeqrent.WithIntro(freeqrent.IntroRipple),
//	))
//
// A style is a Style document (schema/freeqrent.schema.json). Give it as a
// typed [Style], built with [NewStyle] and the With options or as a struct
// literal, or as a raw document with [Document] (JSON text or a
// map[string]any). Unset fields are left out of the document, so the core's
// defaults apply.
//
// Errors are *[Error] values with a stable Code, an English Message and the
// fields in Detail; match them with errors.As.
//
// There are two builds. The default uses cgo and links the prebuilt static
// library lib/<goos>_<goarch>/libfreeqrent_ffi.a (a C toolchain is needed,
// Rust isn't). With -tags freeqrent_wasm the module runs an embedded
// WebAssembly build of the same library on wazero instead, in pure Go
// (CGO_ENABLED=0 works), a few times slower. Both give the same bytes, and
// everything here is safe for concurrent use.
package freeqrent

//go:generate go run ./internal/gen/cmd -schema ../../schema/freeqrent.schema.json -out style_gen.go
