package freeqrent

import (
	"encoding/json"
	"errors"
	"image"
	"image/color"
	"strings"
	"sync"
	"testing"
	"time"
)

const url = "https://example.com/conformance"

const logoSVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a3302a"/><path d="M3 5h4" stroke="#fff"/></svg>`

func mustRender(t *testing.T, text string, style StyleDoc) string {
	t.Helper()
	svg, err := Render(text, style)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(svg, "<svg") {
		t.Fatalf("not an SVG: %.60s", svg)
	}
	return svg
}

// A typed style renders the same bytes as the JSON document it stands for.
func TestTypedStylesMatchTheirJSON(t *testing.T) {
	// A sunset: warm light at the top, deep blue at the bottom.
	sunset := image.NewNRGBA(image.Rect(0, 0, 16, 16))
	for y := 0; y < 16; y++ {
		for x := 0; x < 16; x++ {
			f := float64(y) / 15
			sunset.Set(x, y, color.NRGBA{uint8(250 - 220*f), uint8(220 - 180*f), uint8(160 - 70*f), 255})
		}
	}
	cases := []struct {
		name  string
		typed Style
		doc   string
	}{
		{"options", NewStyle(
			WithPreset("candy"),
			WithModules(Dots{Size: Ptr(0.8)}),
			WithIntro(IntroRipple),
		), `{"preset":"candy","modules":{"kind":"dots","size":0.8},"intro":"ripple"}`},
		{"literal", Style{
			Preset:  PresetInk,
			Modules: Bars{Axis: AxisVertical, Radius: Ptr(0.4)},
			Finders: &Finders{
				Ring:    RingRounded{Radius: Ptr(1.2)},
				Eye:     EyeCircle{},
				Corners: &Corners{TopLeft: &RingAndEye{Ring: RingLeaf{}, Eye: EyeLeaf{}}},
			},
			Alignment: &Alignment{Shape: AlignmentShapeOwn{Ring: RingSquircle{}}, Emphasis: EmphasisAccent},
			Timing:    TimingDotted,
		}, `{"preset":"ink","modules":{"kind":"bars","axis":"vertical","radius":0.4},
			"finders":{"ring":{"kind":"rounded","radius":1.2},"eye":"circle","corners":{"top-left":{"ring":"leaf","eye":"leaf"}}},
			"alignment":{"shape":{"kind":"own","ring":"squircle"},"emphasis":"accent"},"timing":"dotted"}`},
		{"tokens", NewStyle(
			WithToken(TokenFG, "#1d3b72"),
			WithToken(TokenBG, "#f8f5ee"),
			WithFrame(FrameRounded{Radius: Ptr(3.0)}),
			WithBorder(BorderDouble{}),
			WithShadow(1),
			WithTextures(TextureGrain{Amount: Ptr(0.05)}, TextureScanlines{}),
			WithEffect(EffectGlow{Radius: Ptr(0.2), Strength: Ptr(0.0)}),
		), `{"palette":{"fg":"#1d3b72","bg":"#f8f5ee"},"frame":{"kind":"rounded","radius":3},"border":"double",
			"shadow":1,"textures":[{"kind":"grain","amount":0.05},"scanlines"],"effect":{"kind":"glow","radius":0.2,"strength":0}}`},
		{"caption text", NewStyle(WithCaption(CaptionText("scan me"))), `{"caption":"scan me"}`},
		{"caption options", NewStyle(WithCaption(CaptionOptions{Text: "$ scan", Font: FontMono, Bold: Ptr(false), Tracking: Ptr[uint8](10)})),
			`{"caption":{"text":"$ scan","font":"mono","bold":false,"tracking":10}}`},
		{"logo", NewStyle(
			WithLogo(Logo{SVG: Ptr(logoSVG), Frame: LogoFrameOrbit{Width: Ptr(0.6)}, Shape: LogoShapeCircle{}}),
			WithIntro(IntroOptions{Preset: IntroSweep, Logo: LogoIntroSpin, LogoTiming: LogoTimingWith, Ease: EaseCustom{X1: 0.2, Y1: 0, X2: 0, Y2: 1}}),
			WithIdle(IdleLogoGlint),
			WithEncoding(Encoding{ECC: ECCQ}),
		), `{"logo":{"svg":` + jsonString(logoSVG) + `,"frame":{"kind":"orbit","width":0.6},"shape":"circle"},
			"intro":{"preset":"sweep","logo":"spin","logo_timing":"with","ease":{"kind":"custom","x1":0.2,"y1":0,"x2":0,"y2":1}},
			"idle":["logo-glint"],"encoding":{"ecc":"Q"}}`},
		{"palette shift", NewStyle(
			WithIntro(PaletteShift{PaletteFrom: Palette{TokenFG: "#c2410c"}, DurationMS: Ptr[uint32](900), Trigger: TriggerHover{}}),
		), `{"intro":{"palette_from":{"fg":"#c2410c"},"duration_ms":900,"trigger":"hover"}}`},
		{"halftone", NewStyle(
			WithHalftone(Halftone{Image: ImageOf(sunset), Ink: InkColor{Colors: Ptr[uint8](3)}, Bands: &[2]float64{0.25, 0.85}, Clarity: ClarityLevel(0.5)}),
		), `{"halftone":{"image":` + mustJSON(t, ImageOf(sunset)) + `,"ink":{"kind":"color","colors":3},"bands":[0.25,0.85],"clarity":0.5}}`},
		{"encoding", NewStyle(
			WithEncoding(Encoding{ECC: ECCH, Version: Ptr[uint8](10), Mask: Ptr[uint8](3), BoostECC: Ptr(false)}),
			WithSeed(1<<40), WithQuietZone(6), WithMinify(true), WithTitle("A code"), WithIDPrefix("q1"),
			WithTargetSize(TargetSizeMillimeters{Mm: 25}),
		), `{"encoding":{"ecc":"H","version":10,"mask":3,"boost_ecc":false},"seed":1099511627776,"quiet_zone":6,
			"minify":true,"title":"A code","id_prefix":"q1","target_size":{"kind":"millimeters","mm":25}}`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			typed := mustRender(t, url, c.typed)
			raw := mustRender(t, url, Document(c.doc))
			if typed != raw {
				j, _ := c.typed.StyleJSON()
				t.Fatalf("the typed style renders differently from its JSON\n typed: %s\n  json: %s", j, c.doc)
			}
			// And reads back into the same document.
			j, err := c.typed.StyleJSON()
			if err != nil {
				t.Fatal(err)
			}
			var back Style
			if err := json.Unmarshal(j, &back); err != nil {
				t.Fatal(err)
			}
			j2, _ := back.StyleJSON()
			if string(j) != string(j2) {
				t.Fatalf("round trip changed the document\n%s\n%s", j, j2)
			}
		})
	}
}

func jsonString(s string) string {
	b, _ := json.Marshal(s)
	return string(b)
}

func mustJSON(t *testing.T, v any) string {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// An unset field is left out of the document; zero is still sayable.
func TestUnsetFieldsAreOmitted(t *testing.T) {
	j, err := Style{}.StyleJSON()
	if err != nil || string(j) != "{}" {
		t.Fatalf("got %s, %v", j, err)
	}
	j, _ = NewStyle(WithModules(Square{Gap: Ptr(0.0)}), WithTextures()).StyleJSON()
	if string(j) != `{"modules":{"kind":"square","gap":0},"textures":[]}` {
		t.Fatalf("got %s", j)
	}
	var nilStyle StyleDoc
	if a, b := mustRender(t, url, nilStyle), mustRender(t, url, Style{}); a != b {
		t.Fatal("a nil style isn't the default style")
	}
	if a, b := mustRender(t, url, Document(nil)), mustRender(t, url, Document(map[string]any{})); a != b {
		t.Fatal("Document(nil) isn't {}")
	}
}

func TestDocumentForms(t *testing.T) {
	want := mustRender(t, url, NewStyle(WithPreset(PresetSunset)))
	for name, d := range map[string]StyleDoc{
		"string": Document(`{"preset":"sunset"}`),
		"bytes":  Document([]byte(`{"preset":"sunset"}`)),
		"raw":    Document(json.RawMessage(`{"preset":"sunset"}`)),
		"map":    Document(map[string]any{"preset": "sunset"}),
		"struct": Document(struct {
			Preset string `json:"preset"`
		}{"sunset"}),
		"pointer":  &Style{Preset: PresetSunset},
		"styledoc": Document(Style{Preset: PresetSunset}),
	} {
		if got := mustRender(t, url, d); got != want {
			t.Errorf("%s: renders differently", name)
		}
	}
}

func TestStrictUnmarshal(t *testing.T) {
	var s Style
	for _, doc := range []string{
		`{"shape":"dots"}`,
		`{"modules":{"kind":"blob"}}`,
		`{"modules":{"kind":"dots","radius":1}}`,
		`{"intro":{"preset":"ripple","colour":1}}`,
		`{"target_size":"pixels"}`,
		`{"intro":{"trigger":{"kind":"every"}}}`,
	} {
		if err := json.Unmarshal([]byte(doc), &s); err == nil {
			t.Errorf("%s: read without an error", doc)
		}
	}
	// Bare names and objects read alike.
	if err := json.Unmarshal([]byte(`{"modules":"dots","finders":{"eye":{"kind":"circle"}},"intro":"lock-on"}`), &s); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Modules.(Dots); !ok || s.Intro != IntroLockOn {
		t.Fatalf("got %#v", s)
	}
}

func TestErrors(t *testing.T) {
	picture := image.NewNRGBA(image.Rect(0, 0, 8, 8))
	for i := range picture.Pix {
		picture.Pix[i] = 90
	}
	cases := []struct {
		code   string
		text   string
		style  StyleDoc
		detail map[string]any
	}{
		{"invalid_style", "hi", Document(`{"intro":{"trigger":{"kind":"every","ms":"x"}}}`),
			map[string]any{"path": "intro.trigger.ms"}},
		{"invalid_style", "hi", Document(`{"preset":`), map[string]any{"path": ""}},
		{"invalid_style", "hi", Document(`{"shape":"dots"}`), map[string]any{"path": "shape"}},
		{"invalid_logo", "hi", NewStyle(WithLogo(Logo{SVG: Ptr("<html></html>")})),
			map[string]any{"path": "logo.svg", "reason": "not_svg"}},
		{"invalid_image", "hi", Document(`{"halftone":{"image":{"width":2,"height":2,"rgba":"AAAA"}}}`),
			map[string]any{"path": "halftone.image", "expected": 16.0, "actual": 3.0}},
		{"invalid_argument", "\xff", nil, map[string]any{}},
		{"invalid_argument", "hi", Document("{\"title\":\"a\x00\"}"), map[string]any{}},
		{"data_too_long", strings.Repeat("x", 5000), NewStyle(WithEncoding(Encoding{ECC: ECCH})),
			map[string]any{"ecc": "H", "max_version": 40.0}},
		{"invalid_version_range", "hi", NewStyle(WithEncoding(Encoding{MinVersion: Ptr[uint8](10), MaxVersion: Ptr[uint8](5)})),
			map[string]any{"min": 10.0, "max": 5.0}},
		{"contrast_too_low", "hi", NewStyle(WithToken(TokenFG, "#ccc")), map[string]any{"token": "fg"}},
		{"inverted_colors", "hi", NewStyle(WithToken(TokenFG, "#fff"), WithToken(TokenBG, "#000")), map[string]any{"token": "fg"}},
		{"quiet_zone_too_small", "hi", NewStyle(WithQuietZone(1)), map[string]any{}},
		{"option_out_of_range", "hi", NewStyle(WithModules(Dots{Size: Ptr(5.0)})),
			map[string]any{"option": "Dots.size", "value": 5.0, "min": 0.6, "max": 1.0}},
		{"logo_too_large", "hi", NewStyle(WithLogo(Logo{SVG: Ptr(logoSVG), Scale: Ptr(0.5)})), map[string]any{}},
		{"logo_over_alignment", "hi", NewStyle(
			WithHalftone(Halftone{Image: ImageOf(picture)}),
			WithLogo(Logo{SVG: Ptr(`<svg viewBox="0 0 1 1"/>`), Scale: Ptr(0.1)}),
			WithEncoding(Encoding{Version: Ptr[uint8](7), ECC: ECCH}),
		), map[string]any{}},
		{"turn_needs_round_frame", "hi", NewStyle(WithLogo(Logo{SVG: Ptr(logoSVG)}), WithIdle(IdleLogoTurn)), map[string]any{}},
		{"replay_too_soon", "hi", NewStyle(WithIntro(IntroOptions{Preset: IntroRipple, DurationMS: Ptr[uint32](1000), Trigger: TriggerEvery{MS: 1500}})),
			map[string]any{"every_ms": 1500.0, "intro_ms": 1000.0}},
		{"shadow_needs_background", "hi", NewStyle(WithFrame(FrameNone{}), WithShadow(1)), map[string]any{}},
	}
	for _, c := range cases {
		t.Run(c.code, func(t *testing.T) {
			_, err := Render(c.text, c.style)
			var e *Error
			if !errors.As(err, &e) {
				t.Fatalf("want an *Error, got %v", err)
			}
			if e.Code != c.code || e.Message == "" || e.Detail == nil {
				t.Fatalf("got %+v", e)
			}
			for k, v := range c.detail {
				if e.Detail[k] != v {
					t.Errorf("detail[%q] = %#v, want %#v (detail %v)", k, e.Detail[k], v, e.Detail)
				}
			}
			if !errors.Is(err, &Error{Code: c.code}) || errors.Is(err, &Error{Code: "other"}) {
				t.Error("errors.Is doesn't match on the code")
			}
			if !strings.Contains(err.Error(), c.code) {
				t.Errorf("Error() = %q", err.Error())
			}
		})
	}
	// Palette errors, and arguments the Go side refuses.
	_, err := PaletteFromCSS("", PaletteOpts{})
	var e *Error
	if !errors.As(err, &e) || e.Code != "no_colors" {
		t.Errorf("empty CSS: %v", err)
	}
	if _, err := PaletteFromImage(nil, -1, 1, PaletteOpts{}); !errors.As(err, &e) || e.Code != "invalid_argument" {
		t.Errorf("negative width: %v", err)
	}
	if _, err := PaletteFromImage(make([]byte, 3), 1, 1, PaletteOpts{}); !errors.As(err, &e) || e.Code != "invalid_image" {
		t.Errorf("short pixels: %v", err)
	}
	if _, err := RenderFrame(url, nil, -time.Millisecond); !errors.As(err, &e) || e.Code != "invalid_argument" {
		t.Errorf("negative frame time: %v", err)
	}
	if _, err := Render(url, Document(func() {})); err == nil || errors.As(err, &e) {
		t.Errorf("an unmarshalable document: %v", err)
	}
}

func TestBytesPayloads(t *testing.T) {
	data := []byte{0x00, 0xff, 0xfe, 0x80, 'q', 'r'}
	svg, err := RenderBytes(data, NewStyle(WithPreset(PresetTerminal)))
	if err != nil || !strings.HasPrefix(svg, "<svg") {
		t.Fatalf("%v %.40s", err, svg)
	}
	// The same bytes as text aren't UTF-8.
	var e *Error
	if _, err := Render(string(data), nil); !errors.As(err, &e) || e.Code != "invalid_argument" {
		t.Fatalf("got %v", err)
	}
	// ASCII as bytes is byte mode, which isn't always what text picks.
	r, err := CheckBytes([]byte("HELLO WORLD 123"), nil)
	if err != nil || !r.OK {
		t.Fatalf("%v %+v", err, r)
	}
	if _, err := RenderBytes(nil, nil); err != nil {
		t.Fatalf("an empty payload: %v", err)
	}
	frame, err := RenderFrameBytes(data, NewStyle(WithIntro(IntroSettle)), 100*time.Millisecond)
	if err != nil || strings.Contains(frame, "@keyframes") {
		t.Fatalf("%v", err)
	}
	plan, err := PlanBytes(data, NewStyle(WithIntro(IntroSettle)))
	if err != nil || !json.Valid(plan) {
		t.Fatalf("%v", err)
	}
}

func TestCheckReport(t *testing.T) {
	r, err := Check(url, NewStyle(WithPreset(PresetTerminal), WithModules(Dots{Size: Ptr(0.7)})))
	if err != nil {
		t.Fatal(err)
	}
	if !r.OK || len(r.Warnings) == 0 || r.Code.Version == 0 || r.Code.Size != 17+4*r.Code.Version || r.Err() != nil {
		t.Fatalf("%+v", r)
	}
	r, err = Check(url, NewStyle(WithToken(TokenFG, "#bbb")))
	if err != nil {
		t.Fatal(err)
	}
	var e *Error
	if r.OK || !errors.As(r.Err(), &e) || e.Code != "contrast_too_low" {
		t.Fatalf("%+v", r)
	}
	// A report for a style render refuses, but not for a document that's wrong.
	if _, err := Check(url, Document(`{"shape":1}`)); err == nil {
		t.Fatal("a bad document got a report")
	}
}

func TestPlan(t *testing.T) {
	p, err := Plan(url, NewStyle(WithIntro(IntroRipple)))
	if err != nil {
		t.Fatal(err)
	}
	var plan map[string]any
	if err := json.Unmarshal(p, &plan); err != nil || len(plan) == 0 {
		t.Fatalf("the plan isn't JSON: %v (%.80s…)", err, p)
	}
}

func TestPalettes(t *testing.T) {
	css, err := PaletteFromCSS(":root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }", PaletteOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if css.Palette[TokenFG] == "" || len(css.Swatches) == 0 || len(css.Choices) == 0 {
		t.Fatalf("%+v", css)
	}
	for _, c := range css.Choices {
		if c.Origin == nil {
			t.Fatalf("a choice without an origin: %+v", c)
		}
	}
	// The palette drops into a style.
	mustRender(t, url, NewStyle(WithPreset(PresetInk), WithPalette(css.Palette)))

	html, err := PaletteFromHTML(`<body style="background:#fffaf0;color:#2b2118"></body>`, PaletteOpts{LogoColors: []string{"#a3302a"}})
	if err != nil || html.Palette[TokenBG] == "" {
		t.Fatalf("%v %+v", err, html)
	}

	// An image.Image, in any color model, reads like its RGBA bytes.
	rgba := image.NewRGBA(image.Rect(0, 0, 16, 16))
	for y := 0; y < 16; y++ {
		for x := 0; x < 16; x++ {
			c := color.RGBA{0xfa, 0xdc, 0xa0, 0xff}
			if x >= 8 {
				c = color.RGBA{0x1d, 0x3b, 0x72, 0xff}
			}
			rgba.Set(x, y, c)
		}
	}
	fromImage, err := PaletteFromImageOf(rgba, PaletteOpts{Clusters: Ptr[uint8](3), Seed: Ptr[uint64](7)})
	if err != nil {
		t.Fatal(err)
	}
	im := ImageOf(rgba.SubImage(rgba.Bounds()))
	fromBytes, err := PaletteFromImage(im.RGBA, int(im.Width), int(im.Height), PaletteOpts{Clusters: Ptr[uint8](3), Seed: Ptr[uint64](7)})
	if err != nil {
		t.Fatal(err)
	}
	if mustJSON(t, fromImage) != mustJSON(t, fromBytes) {
		t.Fatal("PaletteFromImageOf and PaletteFromImage disagree")
	}
	sub := ImageOf(rgba.SubImage(image.Rect(8, 0, 16, 4)))
	if sub.Width != 8 || sub.Height != 4 || len(sub.RGBA) != 8*4*4 || sub.RGBA[0] != 0x1d {
		t.Fatalf("a sub-image: %d×%d, %d bytes", sub.Width, sub.Height, len(sub.RGBA))
	}
}

func TestPresetsAndVersion(t *testing.T) {
	names, err := Presets()
	if err != nil {
		t.Fatal(err)
	}
	if len(names) < 8 || names[0] != PresetClassic {
		t.Fatalf("%v", names)
	}
	for _, n := range names {
		mustRender(t, "hi", NewStyle(WithPreset(n)))
	}
	if Version() != SchemaVersion {
		t.Fatalf("version %q", Version())
	}
	if b := Backend(); b != "cgo" && b != "wasm" {
		t.Fatalf("backend %q", b)
	}
}

// Goroutines rendering at once get what one goroutine gets.
func TestConcurrentCallsAgree(t *testing.T) {
	styles := []StyleDoc{
		nil,
		NewStyle(WithPreset(PresetCandy), WithIntro(IntroRipple)),
		NewStyle(WithPreset(PresetInk), WithLogo(Logo{SVG: Ptr(logoSVG)}), WithEncoding(Encoding{ECC: ECCQ})),
		NewStyle(WithPreset(PresetSunset), WithTextures(TextureInk{})),
	}
	want := make([]string, len(styles))
	for i, s := range styles {
		want[i] = mustRender(t, url, s)
	}
	const workers, rounds = 16, 8
	var wg sync.WaitGroup
	errs := make(chan error, workers*rounds)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for r := 0; r < rounds; r++ {
				i := (w + r) % len(styles)
				got, err := Render(url, styles[i])
				if err != nil {
					errs <- err
					continue
				}
				if got != want[i] {
					errs <- errors.New("a concurrent render differs")
				}
				if _, err := Check(url, styles[i]); err != nil {
					errs <- err
				}
				if _, err := Render(url, Document(`{"palette":{"fg":"#eee"}}`)); err == nil {
					errs <- errors.New("a refused style rendered")
				}
			}
		}(w)
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		t.Error(err)
	}
}
