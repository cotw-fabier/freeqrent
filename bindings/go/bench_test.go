package freeqrent

import (
	"strings"
	"testing"
)

// A version 10 code (57×57 modules) in the ink preset: compare the builds
// with
//
//	go test -run '^$' -bench . -benchtime 2s
//	CGO_ENABLED=0 go test -tags freeqrent_wasm -run '^$' -bench . -benchtime 2s
func BenchmarkRenderV10(b *testing.B) {
	style := NewStyle(WithPreset(PresetInk), WithEncoding(Encoding{Version: Ptr[uint8](10), ECC: ECCM}))
	text := "https://example.com/" + strings.Repeat("a", 150)
	r, err := Check(text, style)
	if err != nil || r.Code.Version != 10 {
		b.Fatalf("%v version %d", err, r.Code.Version)
	}
	b.ResetTimer()
	for b.Loop() {
		if _, err := Render(text, style); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkRenderV10Parallel(b *testing.B) {
	style := NewStyle(WithPreset(PresetInk), WithEncoding(Encoding{Version: Ptr[uint8](10), ECC: ECCM}))
	text := "https://example.com/" + strings.Repeat("a", 150)
	b.RunParallel(func(pb *testing.PB) {
		for pb.Next() {
			if _, err := Render(text, style); err != nil {
				b.Error(err)
				return
			}
		}
	})
}
