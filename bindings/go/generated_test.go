package freeqrent

import (
	"bytes"
	"os"
	"testing"

	"github.com/cotw-fabier/freeqrent/bindings/go/internal/gen"
)

// style_gen.go is what the generator writes for the checked-in schema.
func TestGeneratedStylesAreFresh(t *testing.T) {
	schema, err := os.ReadFile("../../schema/freeqrent.schema.json")
	if err != nil {
		t.Fatal(err)
	}
	want, err := gen.Generate(schema)
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile("style_gen.go")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatal("style_gen.go is stale: run go generate in bindings/go")
	}
	if SchemaVersion != Version() {
		t.Fatalf("the types are for schema %s, the library is %s", SchemaVersion, Version())
	}
}

// include/freeqrent.h is a copy of the C ABI's header, for cgo.
func TestHeaderMatchesTheFFICrate(t *testing.T) {
	want, err := os.ReadFile("../../crates/freeqrent-ffi/include/freeqrent.h")
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile("include/freeqrent.h")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatal("include/freeqrent.h differs from crates/freeqrent-ffi/include/freeqrent.h: copy it again")
	}
}
