package freeqrent

import (
	"encoding/json"
	"maps"
)

// A StyleDoc is anything that gives a Style document: a [Style], or what
// [Document] returns. A nil StyleDoc is the default style, "{}".
type StyleDoc interface {
	// StyleJSON returns the Style document as JSON text.
	StyleJSON() ([]byte, error)
}

// StyleJSON returns the style as a Style document. Unset fields are left
// out, so the core's defaults (or the preset's) apply.
func (s Style) StyleJSON() ([]byte, error) { return json.Marshal(s) }

// NewStyle returns a Style with each option applied in order.
func NewStyle(opts ...Option) Style {
	var s Style
	for _, o := range opts {
		o(&s)
	}
	return s
}

// With returns a copy of the style with more options applied. The copy is
// shallow, but the options never change what the original points to.
func (s Style) With(opts ...Option) Style {
	for _, o := range opts {
		o(&s)
	}
	return s
}

// WithToken sets one palette token, keeping the others: the --token flag.
// Tokens merge onto the base style's palette in the core, too.
func WithToken(token Token, paint string) Option {
	return func(s *Style) {
		p := make(Palette, len(s.Palette)+1)
		maps.Copy(p, s.Palette)
		p[token] = paint
		s.Palette = p
	}
}

// Ptr returns a pointer to v, for the optional fields of a style literal:
// Dots{Size: freeqrent.Ptr(0.8)}. (From Go 1.26, new(0.8) does the same.)
func Ptr[T any](v T) *T { return &v }

type document struct {
	data []byte
	err  error
}

func (d document) StyleJSON() ([]byte, error) { return d.data, d.err }

// Document returns a raw Style document. Text (a string, []byte or
// json.RawMessage) is passed through as it is; anything else, such as a
// map[string]any, is marshalled with encoding/json first. A nil value is
// the default style.
//
//	freeqrent.Render(text, freeqrent.Document(`{"preset":"ink"}`))
//	freeqrent.Render(text, freeqrent.Document(map[string]any{"preset": "ink"}))
func Document(v any) StyleDoc {
	switch d := v.(type) {
	case nil:
		return document{data: []byte("{}")}
	case string:
		return document{data: []byte(d)}
	case []byte:
		return document{data: append([]byte(nil), d...)}
	case json.RawMessage:
		return document{data: append([]byte(nil), d...)}
	case StyleDoc:
		return d
	}
	data, err := json.Marshal(v)
	return document{data: data, err: err}
}
