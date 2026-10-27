package freeqrent_test

import (
	"errors"
	"fmt"
	"strings"

	"github.com/freeqrent/freeqrent-go"
)

func ExampleRender() {
	svg, err := freeqrent.Render("https://example.com", freeqrent.NewStyle(
		freeqrent.WithPreset(freeqrent.PresetCandy),
		freeqrent.WithModules(freeqrent.Dots{Size: freeqrent.Ptr(0.8)}),
		freeqrent.WithIntro(freeqrent.IntroRipple),
	))
	if err != nil {
		panic(err)
	}
	fmt.Println(strings.HasPrefix(svg, "<svg"))
	// Output: true
}

func ExampleDocument() {
	svg, err := freeqrent.Render("https://example.com", freeqrent.Document(map[string]any{
		"preset": "ink",
		"frame":  map[string]any{"kind": "circle", "ring": "dashed"},
	}))
	fmt.Println(err == nil && strings.HasPrefix(svg, "<svg"))
	// Output: true
}

func ExampleError() {
	_, err := freeqrent.Render("hi", freeqrent.NewStyle(freeqrent.WithToken(freeqrent.TokenFG, "#eee")))
	var e *freeqrent.Error
	if errors.As(err, &e) {
		fmt.Println(e.Code, e.Detail["token"])
	}
	// Output: contrast_too_low fg
}

func ExamplePaletteFromCSS() {
	res, err := freeqrent.PaletteFromCSS(":root { --brand: #1d3b72; --paper: #f8f5ee; }", freeqrent.PaletteOpts{})
	if err != nil {
		panic(err)
	}
	_, err = freeqrent.Render("https://example.com", freeqrent.NewStyle(freeqrent.WithPalette(res.Palette)))
	fmt.Println(err)
	// Output: <nil>
}
