package freeqrent

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"testing"
	"time"
)

// The conformance suite every binding runs (SPEC §10.3).
const casesPath = "../../conformance/cases.json"

type confCase struct {
	Name    string `json:"name"`
	Fn      string `json:"fn"`
	Payload *struct {
		Text  *string `json:"text"`
		Bytes *string `json:"bytes"`
	} `json:"payload"`
	Style json.RawMessage `json:"style"`
	Opts  json.RawMessage `json:"opts"`
	Text  string          `json:"text"`
	Image *struct {
		Width  uint32 `json:"width"`
		Height uint32 `json:"height"`
		RGBA   string `json:"rgba"`
	} `json:"image"`
	TMS    uint32 `json:"t_ms"`
	Expect struct {
		SHA256 string `json:"sha256"`
		Length int    `json:"length"`
		Error  string `json:"error"`
	} `json:"expect"`
}

func loadCases(t testing.TB) []confCase {
	t.Helper()
	data, err := os.ReadFile(casesPath)
	if err != nil {
		t.Fatal(err)
	}
	var suite struct {
		Version string     `json:"version"`
		Cases   []confCase `json:"cases"`
	}
	if err := json.Unmarshal(data, &suite); err != nil {
		t.Fatal(err)
	}
	if suite.Version != Version() {
		t.Fatalf("the suite is for %s, the library is %s", suite.Version, Version())
	}
	return suite.Cases
}

// reserialize decodes a JSON value into Go's generic types and writes it
// back with encoding/json, the way a Go user's map[string]any would reach
// the library: keys sorted, and <, > and & escaped as JSON \u escapes
// (which the document parser reads as the same characters).
func reserialize(t testing.TB, raw json.RawMessage) []byte {
	t.Helper()
	if len(raw) == 0 {
		return []byte("{}")
	}
	var v any
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	out, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return out
}

// run calls the binding for a case, with the style and options given by
// docOf and optsOf, and returns the text the library returned.
func run(t testing.TB, c confCase, style StyleDoc, opts []byte) (string, error) {
	t.Helper()
	switch c.Fn {
	case "render", "render_frame", "check", "plan":
		var data []byte
		kind := kindText
		switch {
		case c.Payload.Text != nil:
			data = []byte(*c.Payload.Text)
		case c.Payload.Bytes != nil:
			b, err := base64.StdEncoding.DecodeString(*c.Payload.Bytes)
			if err != nil {
				t.Fatal(err)
			}
			data, kind = b, kindBytes
		default:
			t.Fatalf("%s: no payload", c.Name)
		}
		fn := map[string]payloadFn{"render": fnRender, "render_frame": fnRenderFrame, "check": fnCheck, "plan": fnPlan}[c.Fn]
		return callPayload(fn, data, kind, style, c.TMS)
	case "palette_from_css":
		return paletteTextRaw(fnPaletteCSS, c.Text, opts)
	case "palette_from_html":
		return paletteTextRaw(fnPaletteHTML, c.Text, opts)
	case "palette_from_image":
		rgba, err := base64.StdEncoding.DecodeString(c.Image.RGBA)
		if err != nil {
			t.Fatal(err)
		}
		return paletteImageRaw(rgba, c.Image.Width, c.Image.Height, opts)
	case "presets":
		return presetsRaw()
	}
	t.Fatalf("%s: unknown fn %q", c.Name, c.Fn)
	return "", nil
}

func expect(t *testing.T, c confCase, out string, err error) {
	t.Helper()
	if c.Expect.Error != "" {
		var e *Error
		if !errors.As(err, &e) {
			t.Fatalf("%s: want error %s, got %v (output %d bytes)", c.Name, c.Expect.Error, err, len(out))
		}
		if e.Code != c.Expect.Error {
			t.Fatalf("%s: want error %s, got %s: %s", c.Name, c.Expect.Error, e.Code, e.Message)
		}
		return
	}
	if err != nil {
		t.Fatalf("%s: %v", c.Name, err)
	}
	sum := sha256.Sum256([]byte(out))
	if got := hex.EncodeToString(sum[:]); got != c.Expect.SHA256 || len(out) != c.Expect.Length {
		t.Fatalf("%s: got sha256 %s length %d, want %s length %d", c.Name, got, len(out), c.Expect.SHA256, c.Expect.Length)
	}
}

func TestConformance(t *testing.T) {
	cases := loadCases(t)
	if len(cases) != 77 {
		t.Errorf("the suite has %d cases; this runner was written for 77", len(cases))
	}
	escaped := 0
	for _, c := range cases {
		t.Run(c.Name, func(t *testing.T) {
			style := reserialize(t, c.Style)
			if bytes.Contains(style, []byte(`\u003c`)) {
				escaped++
			}
			out, err := run(t, c, Document(style), reserialize(t, c.Opts))
			expect(t, c, out, err)
		})
	}
	// The logo cases carry SVG, so Go's HTML escaping is exercised.
	if escaped == 0 {
		t.Error("no case's style was HTML-escaped by encoding/json")
	}
}

// TestConformanceUnescaped sends the same documents without encoding/json's
// HTML escaping, to show the escaping changes nothing.
func TestConformanceUnescaped(t *testing.T) {
	for _, c := range loadCases(t) {
		if c.Style == nil {
			continue
		}
		t.Run(c.Name, func(t *testing.T) {
			var v any
			if err := json.Unmarshal(c.Style, &v); err != nil {
				t.Fatal(err)
			}
			var buf bytes.Buffer
			enc := json.NewEncoder(&buf)
			enc.SetEscapeHTML(false)
			if err := enc.Encode(v); err != nil {
				t.Fatal(err)
			}
			out, err := run(t, c, Document(bytes.TrimSpace(buf.Bytes())), reserialize(t, c.Opts))
			expect(t, c, out, err)
		})
	}
}

// TestConformanceTyped reads every case's document into the generated
// types and sends what they marshal back to: typed styles say the same
// thing as their JSON.
func TestConformanceTyped(t *testing.T) {
	typed := 0
	for _, c := range loadCases(t) {
		t.Run(c.Name, func(t *testing.T) {
			var style StyleDoc
			if c.Style != nil {
				var s Style
				if err := json.Unmarshal(c.Style, &s); err != nil {
					if c.Expect.Error == "invalid_style" {
						return // the types refuse it before the library can
					}
					t.Fatalf("the document doesn't read into Style: %v\n%s", err, c.Style)
				}
				style = s
				typed++
			}
			var opts []byte
			if c.Opts != nil {
				var o PaletteOpts
				if err := json.Unmarshal(c.Opts, &o); err != nil {
					t.Fatalf("the options don't read into PaletteOpts: %v", err)
				}
				var err error
				if opts, err = json.Marshal(o); err != nil {
					t.Fatal(err)
				}
			}
			out, err := run(t, c, style, opts)
			expect(t, c, out, err)
			// The typed results read back too.
			if err == nil {
				switch c.Fn {
				case "check":
					var r Report
					if err := json.Unmarshal([]byte(out), &r); err != nil {
						t.Fatalf("the report doesn't read into Report: %v", err)
					}
				case "palette_from_css", "palette_from_html", "palette_from_image":
					var r PaletteResult
					if err := json.Unmarshal([]byte(out), &r); err != nil {
						t.Fatalf("the palette doesn't read into PaletteResult: %v", err)
					}
				}
			}
		})
	}
	if typed < 70 {
		t.Errorf("only %d cases had a style", typed)
	}
}

// The public functions agree with the runner on the cases they cover.
func TestConformancePublic(t *testing.T) {
	for _, c := range loadCases(t) {
		if c.Expect.Error != "" || c.Payload == nil || c.Payload.Text == nil {
			continue
		}
		doc := Document(reserialize(t, c.Style))
		var out string
		var err error
		switch c.Fn {
		case "render":
			out, err = Render(*c.Payload.Text, doc)
		case "render_frame":
			out, err = RenderFrame(*c.Payload.Text, doc, time.Duration(c.TMS)*time.Millisecond)
		case "plan":
			var p json.RawMessage
			p, err = Plan(*c.Payload.Text, doc)
			out = string(p)
		default:
			continue
		}
		expect(t, c, out, err)
	}
}
