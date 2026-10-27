// Package gen writes freeQRent's typed Go styles (style_gen.go) from the
// Style document's JSON schema, schema/freeqrent.schema.json.
//
// The mapping:
//
//   - An object becomes a struct. A key that isn't required becomes a
//     field omitted when unset (a pointer for numbers, booleans and
//     strings, so zero stays sayable; nil slices and maps; "" for enums,
//     which no enum allows), so the core's defaults apply.
//   - A string enum becomes a named string type with constants.
//   - A union tagged by "kind" becomes a sealed interface with one struct
//     per variant, marshalled with its "kind".
//   - Any other union (an intro, a caption, a clarity) becomes a sealed
//     interface its branches implement, told apart by JSON type and by
//     required keys.
//
// Every struct gets a strict UnmarshalJSON (unknown keys are errors), so a
// document parses back into the types.
package gen

import (
	"bytes"
	"fmt"
	"go/format"
	"sort"
	"strconv"
	"strings"
)

// Names the schema leaves to us: inline objects, unions and enums, by
// "Parent.key", "Parent.key[]" for array items, or "Union|i" for the i-th
// branch of an untagged union.
var names = map[string]string{
	"Finders.corners":          "Corners",
	"Alignment.shape":          "AlignmentShape",
	"Logo.frame":               "LogoFrame",
	"Logo.shape":               "LogoShape",
	"Logo.size":                "LogoSize",
	"Halftone.ink":             "Ink",
	"Halftone.clarity":         "Clarity",
	"Intro|0":                  "IntroPreset",
	"Intro|1":                  "IntroOptions",
	"Intro|2":                  "PaletteShift",
	"IntroOptions.logo":        "LogoIntro",
	"IntroOptions.logo_timing": "LogoTiming",
	"Caption|0":                "CaptionText",
	"Caption|1":                "CaptionOptions",
	"Clarity|0":                "ClarityLevel",
	"FrameCircle.ring":         "FrameRing",
	"Report.code":              "CodeInfo",
	"Report.steering":          "Steering",
	"PaletteResult.swatches[]": "Swatch",
	"PaletteResult.choices[]":  "Choice",
	"Choice.origin":            "Origin",
	"Palette#keys":             "Token",
}

// Variant name prefixes; a union not listed prefixes its own name.
var variantPrefix = map[string]string{"ModuleShape": ""}

// Constant prefixes; an enum not listed prefixes its own name.
var constPrefix = map[string]string{"IntroPreset": "Intro"}

var initialisms = map[string]string{
	"id": "ID", "ms": "MS", "ecc": "ECC", "eci": "ECI", "rgba": "RGBA", "svg": "SVG", "ok": "OK", "fg": "FG", "bg": "BG",
}

func pascal(s string) string {
	var b strings.Builder
	for _, w := range strings.FieldsFunc(s, func(r rune) bool {
		return r == '_' || r == '-' || r == '.' || r == ' '
	}) {
		if up, ok := initialisms[w]; ok {
			b.WriteString(up)
			continue
		}
		b.WriteString(strings.ToUpper(w[:1]) + w[1:])
	}
	return b.String()
}

type cat int

const (
	catScalar cat = iota // bool, string, numbers
	catEnum
	catStruct
	catIface
	catBytes
	catMap
	catSlice
	catPair // [2]float64
)

type ref struct {
	goType   string
	cat      cat
	nullable bool
	elem     *ref // catSlice
}

type field struct {
	goName, key, doc string
	t                ref
	required         bool
}

type structT struct {
	name, doc  string
	fields     []field
	kind       string // a tagged variant's kind
	unionName  string
	implements []string
	lenient    bool // a result type: unknown keys are skipped
}

type enumT struct {
	name, doc, prefix string
	values            []string
	implements        []string
}

type scalarT struct { // a named string or number branch
	name, base, doc string
	implements      []string
}

type variant struct {
	kind, typeName string
	bare           bool
}

type branch struct {
	json     byte // 's' string, 'n' number, 'o' object
	typeName string
	required []string
}

type unionT struct {
	name, doc string
	tagged    bool
	variants  []variant
	branches  []branch
}

type gen struct {
	defs     *node
	taken    map[string]bool
	resolved map[string]*ref
	decls    []any
	enums    map[string]*enumT // by joined values
	byName   map[string]any
	lenient  bool // resolving a result type
}

// What comes back from the library; a later 1.x may add keys to these, so
// they read leniently.
var results = map[string]bool{"Error": true, "Report": true, "PaletteResult": true}

// Generate returns the gofmt'ed Go source for the schema.
func Generate(schema []byte) ([]byte, error) {
	root, err := parse(schema)
	if err != nil {
		return nil, fmt.Errorf("parsing the schema: %w", err)
	}
	g := &gen{
		defs:     root.get("$defs"),
		taken:    map[string]bool{},
		resolved: map[string]*ref{},
		enums:    map[string]*enumT{},
		byName:   map[string]any{},
	}
	if g.defs == nil {
		return nil, fmt.Errorf("the schema has no $defs")
	}
	for _, k := range g.defs.keys {
		g.taken[k] = true
	}
	for _, k := range g.defs.keys {
		g.lenient = results[k]
		if _, err := g.def(k); err != nil {
			return nil, err
		}
	}
	version := ""
	if v := root.get("x-freeqrent-version"); v != nil {
		version = v.str
	}
	src := g.emit(version)
	out, err := format.Source(src)
	if err != nil {
		return nil, fmt.Errorf("formatting: %w\n%s", err, src)
	}
	return out, nil
}

func (g *gen) claim(name string) error {
	if g.byName[name] != nil {
		return fmt.Errorf("two types named %s", name)
	}
	g.taken[name] = true
	return nil
}

func (g *gen) add(name string, d any) error {
	if err := g.claim(name); err != nil {
		return err
	}
	g.byName[name] = d
	g.decls = append(g.decls, d)
	return nil
}

func (g *gen) def(name string) (*ref, error) {
	if r, ok := g.resolved[name]; ok {
		if r == nil {
			return &ref{goType: name, cat: catStruct}, nil // a cycle; structs only
		}
		return r, nil
	}
	n := g.defs.get(name)
	if n == nil {
		return nil, fmt.Errorf("no $defs/%s", name)
	}
	g.resolved[name] = nil
	var r *ref
	var err error
	switch {
	case n.get("contentEncoding") != nil && n.get("contentEncoding").str == "base64":
		r = &ref{goType: "[]byte", cat: catBytes}
	case n.has("propertyNames"):
		r, err = g.paletteMap(name, n)
	default:
		r, err = g.named(n, name, "", "")
	}
	if err != nil {
		return nil, fmt.Errorf("$defs/%s: %w", name, err)
	}
	g.resolved[name] = r
	return r, nil
}

func (g *gen) paletteMap(name string, n *node) (*ref, error) {
	key, err := g.enum(n.get("propertyNames"), []string{names[name+"#keys"]}, "")
	if err != nil {
		return nil, err
	}
	doc := "token paints"
	if d := n.get("additionalProperties").get("description"); d != nil {
		doc = "token: " + d.str
	}
	if err := g.add(name, &scalarT{name: name, base: "map[" + key.goType + "]string", doc: doc}); err != nil {
		return nil, err
	}
	return &ref{goType: name, cat: catMap}, nil
}

func desc(n *node) string {
	if d := n.get("description"); d != nil {
		return d.str
	}
	return ""
}

// named resolves a node that becomes (or is) a named type called name.
func (g *gen) named(n *node, name, parent, key string) (*ref, error) {
	if r := n.get("$ref"); r != nil {
		return g.def(strings.TrimPrefix(r.str, "#/$defs/"))
	}
	if one := n.get("oneOf"); one != nil && !n.has("properties") {
		return g.union(n, name)
	}
	if n.has("enum") {
		return g.enum(n, []string{name}, desc(n))
	}
	if t := n.get("type"); t != nil && t.kind == 's' && t.str == "object" {
		if n.has("properties") {
			if err := g.structure(n, name, "", ""); err != nil {
				return nil, err
			}
			return &ref{goType: name, cat: catStruct}, nil
		}
		return &ref{goType: "map[string]any", cat: catMap}, nil
	}
	return nil, fmt.Errorf("%s: can't name this node", name)
}

func (g *gen) enum(n *node, candidates []string, doc string) (*ref, error) {
	values := n.get("enum").strs()
	sig := strings.Join(values, "\x00")
	if e, ok := g.enums[sig]; ok {
		return &ref{goType: e.name, cat: catEnum}, nil
	}
	name := ""
	for _, c := range candidates {
		if c != "" && !g.taken[c] {
			name = c
			break
		}
	}
	if name == "" {
		return nil, fmt.Errorf("no free name for the enum %v (tried %v)", values, candidates)
	}
	prefix, ok := constPrefix[name]
	if !ok {
		prefix = name
	}
	e := &enumT{name: name, doc: doc, prefix: prefix, values: values}
	if err := g.add(name, e); err != nil {
		return nil, err
	}
	g.enums[sig] = e
	return &ref{goType: name, cat: catEnum}, nil
}

func (g *gen) structure(n *node, name, kind, union string) error {
	s := &structT{name: name, doc: desc(n), kind: kind, unionName: union, lenient: g.lenient}
	// Claim the name first so nested types can't take it.
	if err := g.add(name, s); err != nil {
		return err
	}
	required := map[string]bool{}
	for _, r := range n.get("required").strs() {
		required[r] = true
	}
	props := n.get("properties")
	for _, k := range props.keys {
		if k == "kind" && kind != "" {
			continue
		}
		p := props.vals[k]
		t, err := g.prop(p, name, k)
		if err != nil {
			return fmt.Errorf("%s.%s: %w", name, k, err)
		}
		s.fields = append(s.fields, field{goName: pascal(k), key: k, doc: desc(p), t: *t, required: required[k]})
	}
	return nil
}

func intType(n *node) string {
	maxN := n.get("maximum")
	if maxN == nil {
		return "int64"
	}
	m, err := strconv.ParseUint(string(maxN.num), 10, 64)
	if err != nil {
		return "int64"
	}
	switch {
	case m <= 255:
		return "uint8"
	case m <= 4294967295:
		return "uint32"
	default:
		return "uint64"
	}
}

func scalar(typ string, n *node) (*ref, bool) {
	switch typ {
	case "string":
		return &ref{goType: "string"}, true
	case "number":
		return &ref{goType: "float64"}, true
	case "integer":
		return &ref{goType: intType(n)}, true
	case "boolean":
		return &ref{goType: "bool"}, true
	}
	return nil, false
}

// prop resolves the type of parent's key.
func (g *gen) prop(p *node, parent, key string) (*ref, error) {
	path := parent + "." + key
	if r := p.get("$ref"); r != nil {
		return g.def(strings.TrimPrefix(r.str, "#/$defs/"))
	}
	if one := p.get("oneOf"); one != nil {
		// null or X: X, nullable.
		if len(one.arr) == 2 && one.arr[0].get("type") != nil && one.arr[0].get("type").str == "null" {
			r, err := g.prop(one.arr[1], parent, key)
			if err != nil {
				return nil, err
			}
			r.nullable = true
			return r, nil
		}
		name := names[path]
		if name == "" {
			return nil, fmt.Errorf("an inline union needs a name in gen.names[%q]", path)
		}
		return g.union(p, name)
	}
	if p.has("enum") {
		return g.enum(p, []string{names[path], pascal(key), parent + pascal(key)}, desc(p))
	}
	t := p.get("type")
	if t == nil {
		return nil, fmt.Errorf("no type")
	}
	if t.kind == 'a' { // ["number", "null"]
		types := t.strs()
		if len(types) == 2 && types[1] == "null" {
			if r, ok := scalar(types[0], p); ok {
				r.nullable = true
				return r, nil
			}
		}
		return nil, fmt.Errorf("unsupported type list %v", types)
	}
	if r, ok := scalar(t.str, p); ok {
		return r, nil
	}
	switch t.str {
	case "array":
		items := p.get("items")
		if mn, mx := p.get("minItems"), p.get("maxItems"); mn != nil && mx != nil && mn.num == "2" && mx.num == "2" &&
			items.get("type") != nil && items.get("type").str == "number" {
			return &ref{goType: "[2]float64", cat: catPair}, nil
		}
		var elem *ref
		var err error
		if items.get("type") != nil && items.get("type").str == "object" {
			name := names[path+"[]"]
			if name == "" {
				return nil, fmt.Errorf("inline array items need a name in gen.names[%q]", path+"[]")
			}
			elem, err = g.named(items, name, parent, key)
		} else {
			elem, err = g.prop(items, parent, key)
		}
		if err != nil {
			return nil, err
		}
		return &ref{goType: "[]" + elem.goType, cat: catSlice, elem: elem}, nil
	case "object":
		if p.has("properties") {
			name := names[path]
			if name == "" {
				return nil, fmt.Errorf("an inline object needs a name in gen.names[%q]", path)
			}
			if err := g.structure(p, name, "", ""); err != nil {
				return nil, err
			}
			return &ref{goType: name, cat: catStruct}, nil
		}
		return &ref{goType: "map[string]any", cat: catMap}, nil
	}
	return nil, fmt.Errorf("unsupported type %q", t.str)
}

func kindOf(n *node) string {
	k := n.get("properties").get("kind")
	if k == nil || k.get("const") == nil {
		return ""
	}
	return k.get("const").str
}

func (g *gen) union(n *node, name string) (*ref, error) {
	if r, ok := g.byName[name].(*unionT); ok {
		return &ref{goType: r.name, cat: catIface}, nil
	}
	one := n.get("oneOf").arr
	tagged := true
	for _, b := range one {
		if b.has("enum") {
			continue
		}
		if kindOf(b) == "" {
			tagged = false
		}
	}
	u := &unionT{name: name, doc: desc(n), tagged: tagged}
	if err := g.add(name, u); err != nil {
		return nil, err
	}
	if tagged {
		bare := map[string]bool{}
		for _, b := range one {
			if b.has("enum") {
				for _, v := range b.get("enum").strs() {
					bare[v] = true
				}
			}
		}
		prefix, ok := variantPrefix[name]
		if !ok {
			prefix = name
		}
		for _, b := range one {
			if b.has("enum") {
				continue
			}
			kind := kindOf(b)
			vn := prefix + pascal(kind)
			if err := g.structure(b, vn, kind, name); err != nil {
				return nil, err
			}
			u.variants = append(u.variants, variant{kind: kind, typeName: vn, bare: bare[kind]})
		}
		for v := range bare {
			found := false
			for _, x := range u.variants {
				found = found || x.kind == v
			}
			if !found {
				return nil, fmt.Errorf("%s: the bare name %q has no variant", name, v)
			}
		}
		return &ref{goType: name, cat: catIface}, nil
	}
	for i, b := range one {
		key := name + "|" + strconv.Itoa(i)
		var br branch
		switch {
		case b.has("$ref"):
			r, err := g.def(strings.TrimPrefix(b.get("$ref").str, "#/$defs/"))
			if err != nil {
				return nil, err
			}
			if r.cat != catStruct {
				return nil, fmt.Errorf("%s: branch %d isn't a struct", name, i)
			}
			br = branch{json: 'o', typeName: r.goType, required: g.defs.get(r.goType).get("required").strs()}
		case b.has("enum"):
			r, err := g.enum(b, []string{names[key]}, desc(b))
			if err != nil {
				return nil, err
			}
			br = branch{json: 's', typeName: r.goType}
		case b.get("type") != nil && b.get("type").str == "string":
			if names[key] == "" {
				return nil, fmt.Errorf("a union branch needs a name in gen.names[%q]", key)
			}
			if err := g.add(names[key], &scalarT{name: names[key], base: "string", doc: desc(b)}); err != nil {
				return nil, err
			}
			br = branch{json: 's', typeName: names[key]}
		case b.get("type") != nil && b.get("type").str == "number":
			if names[key] == "" {
				return nil, fmt.Errorf("a union branch needs a name in gen.names[%q]", key)
			}
			if err := g.add(names[key], &scalarT{name: names[key], base: "float64", doc: desc(b)}); err != nil {
				return nil, err
			}
			br = branch{json: 'n', typeName: names[key]}
		case b.get("type") != nil && b.get("type").str == "object":
			if names[key] == "" {
				return nil, fmt.Errorf("a union branch needs a name in gen.names[%q]", key)
			}
			if err := g.structure(b, names[key], "", ""); err != nil {
				return nil, err
			}
			br = branch{json: 'o', typeName: names[key], required: b.get("required").strs()}
		default:
			return nil, fmt.Errorf("%s: unsupported branch %d", name, i)
		}
		u.branches = append(u.branches, br)
	}
	seen := map[byte]int{}
	for _, b := range u.branches {
		seen[b.json]++
	}
	if seen['s'] > 1 || seen['n'] > 1 {
		return nil, fmt.Errorf("%s: two branches of one JSON type", name)
	}
	if seen['o'] > 1 {
		// Objects are told apart by required keys: all but one need some.
		without := 0
		for _, b := range u.branches {
			if b.json == 'o' && len(b.required) == 0 {
				without++
			}
		}
		if without > 1 {
			return nil, fmt.Errorf("%s: object branches can't be told apart", name)
		}
	}
	for _, b := range u.branches {
		g.implement(b.typeName, name)
	}
	return &ref{goType: name, cat: catIface}, nil
}

func (g *gen) implement(typeName, iface string) {
	switch d := g.byName[typeName].(type) {
	case *structT:
		d.implements = append(d.implements, iface)
	case *enumT:
		d.implements = append(d.implements, iface)
	case *scalarT:
		d.implements = append(d.implements, iface)
	}
}

// goField is a field's Go type and tag.
func goField(f field) (string, string) {
	t := f.t.goType
	pointer := false
	switch f.t.cat {
	case catScalar, catStruct, catPair:
		pointer = !f.required || f.t.nullable
	case catEnum:
		pointer = f.t.nullable
	}
	if pointer {
		t = "*" + t
	}
	tag := f.key
	if !f.required {
		tag += ",omitzero"
	}
	return t, "`json:\"" + tag + "\"`"
}

func comment(b *bytes.Buffer, indent, text string) {
	if text == "" {
		return
	}
	for _, line := range wrap(text, 74) {
		fmt.Fprintf(b, "%s// %s\n", indent, line)
	}
}

func wrap(text string, width int) []string {
	var lines []string
	line := ""
	for _, w := range strings.Fields(text) {
		if line != "" && len(line)+1+len(w) > width {
			lines = append(lines, line)
			line = w
		} else if line == "" {
			line = w
		} else {
			line += " " + w
		}
	}
	if line != "" {
		lines = append(lines, line)
	}
	return lines
}

func (g *gen) emit(version string) []byte {
	var b bytes.Buffer
	b.WriteString("// Code generated by go run ./internal/gen/cmd from schema/freeqrent.schema.json; DO NOT EDIT.\n\n")
	b.WriteString("package freeqrent\n\nimport (\n\t\"bytes\"\n\t\"encoding/json\"\n\t\"fmt\"\n\t\"sort\"\n)\n\n")
	fmt.Fprintf(&b, "// SchemaVersion is the x-freeqrent-version of the schema these types were generated from.\nconst SchemaVersion = %q\n\n", version)
	for _, d := range g.decls {
		switch d := d.(type) {
		case *enumT:
			g.emitEnum(&b, d)
		case *scalarT:
			comment(&b, "", fmt.Sprintf("%s is %s.", d.name, orElse(d.doc, "a "+d.base)))
			fmt.Fprintf(&b, "type %s %s\n\n", d.name, d.base)
			for _, i := range d.implements {
				fmt.Fprintf(&b, "func (%s) is%s() {}\n\n", d.name, i)
			}
		case *structT:
			g.emitStruct(&b, d)
		case *unionT:
			g.emitUnion(&b, d)
		}
	}
	g.emitOptions(&b)
	b.WriteString(helpers)
	return b.Bytes()
}

func orElse(a, b string) string {
	if a != "" {
		return strings.TrimSuffix(lowerFirst(a), ".")
	}
	return b
}

func lowerFirst(s string) string {
	if s == "" || (len(s) > 1 && s[1] >= 'A' && s[1] <= 'Z') {
		return s
	}
	return strings.ToLower(s[:1]) + s[1:]
}

func (g *gen) emitEnum(b *bytes.Buffer, e *enumT) {
	doc := fmt.Sprintf("%s is one of %s.", e.name, quoteList(e.values))
	if e.doc != "" {
		doc = e.doc + " " + doc
	}
	comment(b, "", doc)
	fmt.Fprintf(b, "type %s string\n\n", e.name)
	fmt.Fprintf(b, "// The values of %s.\nconst (\n", e.name)
	for _, v := range e.values {
		fmt.Fprintf(b, "\t%s%s %s = %q\n", e.prefix, pascal(v), e.name, v)
	}
	b.WriteString(")\n\n")
	for _, i := range e.implements {
		fmt.Fprintf(b, "func (%s) is%s() {}\n\n", e.name, i)
	}
}

func quoteList(vs []string) string {
	q := make([]string, len(vs))
	for i, v := range vs {
		q[i] = strconv.Quote(v)
	}
	if len(q) <= 2 {
		return strings.Join(q, " or ")
	}
	return strings.Join(q[:len(q)-1], ", ") + " or " + q[len(q)-1]
}

func (g *gen) emitStruct(b *bytes.Buffer, s *structT) {
	doc := s.doc
	if s.kind != "" {
		doc = fmt.Sprintf("%s is the %s {\"kind\": %q}. %s", s.name, s.unionName, s.kind, s.doc)
	} else if doc != "" {
		doc = s.name + ": " + lowerFirst(doc)
	} else {
		doc = fmt.Sprintf("%s mirrors the schema's %s.", s.name, s.name)
	}
	comment(b, "", doc)
	fmt.Fprintf(b, "type %s struct {\n", s.name)
	for _, f := range s.fields {
		comment(b, "\t", f.doc)
		t, tag := goField(f)
		fmt.Fprintf(b, "\t%s %s %s\n", f.goName, t, tag)
	}
	b.WriteString("}\n\n")
	if s.unionName != "" {
		fmt.Fprintf(b, "func (%s) is%s() {}\n\n", s.name, s.unionName)
		fmt.Fprintf(b, "// MarshalJSON writes the variant with its \"kind\".\nfunc (v %s) MarshalJSON() ([]byte, error) {\n", s.name)
		fmt.Fprintf(b, "\ttype plain %s\n\treturn json.Marshal(struct {\n\t\tKind string `json:\"kind\"`\n\t\tplain\n\t}{%q, plain(v)})\n}\n\n", s.name, s.kind)
	}
	for _, i := range s.implements {
		fmt.Fprintf(b, "func (%s) is%s() {}\n\n", s.name, i)
	}
	// UnmarshalJSON: strict keys, required keys.
	if s.lenient {
		fmt.Fprintf(b, "// UnmarshalJSON reads the object, skipping keys a later version may add.\nfunc (v *%s) UnmarshalJSON(data []byte) error {\n", s.name)
	} else {
		fmt.Fprintf(b, "// UnmarshalJSON reads the object strictly: unknown keys are errors.\nfunc (v *%s) UnmarshalJSON(data []byte) error {\n", s.name)
	}
	fmt.Fprintf(b, "\tfields, keys, err := objectFields(data, %q)\n\tif err != nil {\n\t\treturn err\n\t}\n", s.name)
	fmt.Fprintf(b, "\t*v = %s{}\n", s.name)
	var req []string
	for _, f := range s.fields {
		if f.required {
			req = append(req, f.key)
		}
	}
	if len(req) > 0 {
		fmt.Fprintf(b, "\tfor _, k := range []string{%s} {\n\t\tif _, ok := fields[k]; !ok {\n\t\t\treturn fmt.Errorf(\"freeqrent: %s needs %%q\", k)\n\t\t}\n\t}\n", quoteJoin(req), s.name)
	}
	b.WriteString("\tfor _, k := range keys {\n\t\traw := fields[k]\n\t\tvar err error\n\t\tswitch k {\n")
	if s.kind != "" {
		fmt.Fprintf(b, "\t\tcase \"kind\":\n\t\t\tvar kind string\n\t\t\tif err = json.Unmarshal(raw, &kind); err == nil && kind != %q {\n\t\t\t\terr = fmt.Errorf(\"%%q isn't %s\", kind)\n\t\t\t}\n", s.kind, s.kind)
	}
	for _, f := range s.fields {
		fmt.Fprintf(b, "\t\tcase %q:\n", f.key)
		switch {
		case f.t.cat == catIface:
			fmt.Fprintf(b, "\t\t\tv.%s, err = decode%s(raw)\n", f.goName, f.t.goType)
		case f.t.cat == catSlice && f.t.elem.cat == catIface:
			fmt.Fprintf(b, "\t\t\tvar items []json.RawMessage\n\t\t\tif err = json.Unmarshal(raw, &items); err == nil {\n")
			fmt.Fprintf(b, "\t\t\t\tv.%s = make(%s, 0, len(items))\n\t\t\t\tfor _, item := range items {\n", f.goName, f.t.goType)
			fmt.Fprintf(b, "\t\t\t\t\tvar x %s\n\t\t\t\t\tif x, err = decode%s(item); err != nil {\n\t\t\t\t\t\tbreak\n\t\t\t\t\t}\n\t\t\t\t\tv.%s = append(v.%s, x)\n\t\t\t\t}\n\t\t\t}\n", f.t.elem.goType, f.t.elem.goType, f.goName, f.goName)
		default:
			fmt.Fprintf(b, "\t\t\terr = json.Unmarshal(raw, &v.%s)\n", f.goName)
		}
	}
	if s.lenient {
		b.WriteString("\t\t}\n")
	} else {
		fmt.Fprintf(b, "\t\tdefault:\n\t\t\terr = fmt.Errorf(\"isn't a key of %s\")\n\t\t}\n", s.name)
	}
	fmt.Fprintf(b, "\t\tif err != nil {\n\t\t\treturn fmt.Errorf(\"freeqrent: %s.%%s: %%w\", k, err)\n\t\t}\n\t}\n\treturn nil\n}\n\n", s.name)
}

func quoteJoin(vs []string) string {
	q := make([]string, len(vs))
	for i, v := range vs {
		q[i] = strconv.Quote(v)
	}
	return strings.Join(q, ", ")
}

func (g *gen) emitUnion(b *bytes.Buffer, u *unionT) {
	var doc string
	if u.tagged {
		var vs, bare []string
		for _, v := range u.variants {
			vs = append(vs, v.typeName)
			if v.bare {
				bare = append(bare, v.kind)
			}
		}
		doc = fmt.Sprintf("%s is one of %s, written as an object tagged by \"kind\".", u.name, strings.Join(vs, ", "))
		if len(bare) > 0 {
			doc += fmt.Sprintf(" A document may also give %s as a bare name.", quoteList(bare))
		}
	} else {
		var vs []string
		for _, br := range u.branches {
			vs = append(vs, br.typeName)
		}
		doc = fmt.Sprintf("%s is one of %s.", u.name, strings.Join(vs, ", "))
	}
	if u.doc != "" {
		doc = u.doc + " " + doc
	}
	comment(b, "", doc)
	fmt.Fprintf(b, "type %s interface {\n\tis%s()\n}\n\n", u.name, u.name)
	fmt.Fprintf(b, "func decode%s(data []byte) (%s, error) {\n", u.name, u.name)
	if u.tagged {
		fmt.Fprintf(b, "\tkind, bare, err := peekKind(data, %q)\n\tif err != nil {\n\t\treturn nil, err\n\t}\n\tswitch kind {\n", u.name)
		for _, v := range u.variants {
			fmt.Fprintf(b, "\tcase %q:\n", v.kind)
			if v.bare {
				fmt.Fprintf(b, "\t\tif bare {\n\t\t\treturn %s{}, nil\n\t\t}\n", v.typeName)
			} else {
				fmt.Fprintf(b, "\t\tif bare {\n\t\t\treturn nil, fmt.Errorf(\"freeqrent: %s %%q needs an object\", kind)\n\t\t}\n", u.name)
			}
			fmt.Fprintf(b, "\t\tvar v %s\n\t\terr := json.Unmarshal(data, &v)\n\t\treturn v, err\n", v.typeName)
		}
		fmt.Fprintf(b, "\t}\n\treturn nil, fmt.Errorf(\"freeqrent: %%q isn't a kind of %s\", kind)\n}\n\n", u.name)
		return
	}
	b.WriteString("\tswitch first(data) {\n")
	for _, j := range []byte{'s', 'n'} {
		for _, br := range u.branches {
			if br.json != j {
				continue
			}
			if j == 's' {
				b.WriteString("\tcase '\"':\n")
			} else {
				b.WriteString("\tcase '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9':\n")
			}
			fmt.Fprintf(b, "\t\tvar v %s\n\t\terr := json.Unmarshal(data, &v)\n\t\treturn v, err\n", br.typeName)
		}
	}
	var objs []branch
	for _, br := range u.branches {
		if br.json == 'o' {
			objs = append(objs, br)
		}
	}
	if len(objs) > 0 {
		b.WriteString("\tcase '{':\n")
		// Branches with required keys first, most keys first.
		sort.SliceStable(objs, func(i, j int) bool { return len(objs[i].required) > len(objs[j].required) })
		if len(objs) > 1 {
			fmt.Fprintf(b, "\t\tfields, _, ferr := objectFields(data, %q)\n\t\tif ferr != nil {\n\t\t\treturn nil, ferr\n\t\t}\n", u.name)
		}
		for i, br := range objs {
			if len(objs) > 1 && (i < len(objs)-1 || len(br.required) > 0) {
				var conds []string
				for _, r := range br.required {
					conds = append(conds, fmt.Sprintf("has(fields, %q)", r))
				}
				fmt.Fprintf(b, "\t\tif %s {\n\t\t\tvar v %s\n\t\t\terr := json.Unmarshal(data, &v)\n\t\t\treturn v, err\n\t\t}\n", strings.Join(conds, " && "), br.typeName)
				continue
			}
			fmt.Fprintf(b, "\t\tvar v %s\n\t\terr := json.Unmarshal(data, &v)\n\t\treturn v, err\n", br.typeName)
		}
	}
	fmt.Fprintf(b, "\t}\n\treturn nil, fmt.Errorf(\"freeqrent: %%s isn't valid for %s\", data)\n}\n\n", u.name)
}

func (g *gen) emitOptions(b *bytes.Buffer) {
	s, ok := g.byName["Style"].(*structT)
	if !ok {
		return
	}
	b.WriteString("// An Option sets one part of a Style; see NewStyle.\ntype Option func(*Style)\n\n")
	for _, f := range s.fields {
		t, _ := goField(f)
		name := "With" + f.goName
		switch {
		case strings.HasPrefix(t, "*"):
			fmt.Fprintf(b, "// %s sets Style.%s.\nfunc %s(v %s) Option {\n\treturn func(s *Style) { s.%s = &v }\n}\n\n", name, f.goName, name, t[1:], f.goName)
		case f.t.cat == catSlice:
			fmt.Fprintf(b, "// %s sets Style.%s; with no values it sends an empty list.\nfunc %s(v ...%s) Option {\n\treturn func(s *Style) { s.%s = append(%s{}, v...) }\n}\n\n", name, f.goName, name, f.t.elem.goType, f.goName, t)
		default:
			fmt.Fprintf(b, "// %s sets Style.%s.\nfunc %s(v %s) Option {\n\treturn func(s *Style) { s.%s = v }\n}\n\n", name, f.goName, name, t, f.goName)
		}
	}
}

const helpers = `
// objectFields splits a JSON object into its fields, with the keys sorted
// so errors come out in a stable order.
func objectFields(data []byte, what string) (map[string]json.RawMessage, []string, error) {
	if first(data) != '{' {
		return nil, nil, fmt.Errorf("freeqrent: %s: %s isn't an object", what, data)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return nil, nil, fmt.Errorf("freeqrent: %s: %w", what, err)
	}
	keys := make([]string, 0, len(fields))
	for k := range fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return fields, keys, nil
}

// peekKind reads a tagged variant's kind, from an object or a bare name.
func peekKind(data []byte, what string) (kind string, bare bool, err error) {
	if first(data) == '"' {
		err = json.Unmarshal(data, &kind)
		return kind, true, err
	}
	fields, _, err := objectFields(data, what)
	if err != nil {
		return "", false, err
	}
	raw, ok := fields["kind"]
	if !ok {
		return "", false, fmt.Errorf("freeqrent: %s needs a \"kind\"", what)
	}
	err = json.Unmarshal(raw, &kind)
	return kind, false, err
}

func has(fields map[string]json.RawMessage, key string) bool {
	_, ok := fields[key]
	return ok
}

func first(data []byte) byte {
	data = bytes.TrimLeft(data, " \t\r\n")
	if len(data) == 0 {
		return 0
	}
	return data[0]
}
`
