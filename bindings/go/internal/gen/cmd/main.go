// Command cmd writes style_gen.go from the Style document's schema:
//
//	go run ./internal/gen/cmd -schema ../../schema/freeqrent.schema.json -out style_gen.go
//
// It is what go generate runs in the module root.
package main

import (
	"flag"
	"fmt"
	"os"

	"github.com/cotw-fabier/freeqrent/bindings/go/internal/gen"
)

func main() {
	schema := flag.String("schema", "../../schema/freeqrent.schema.json", "the Style document's JSON schema")
	out := flag.String("out", "style_gen.go", "the Go file to write")
	flag.Parse()
	data, err := os.ReadFile(*schema)
	if err != nil {
		fmt.Fprintln(os.Stderr, "gen:", err)
		os.Exit(1)
	}
	src, err := gen.Generate(data)
	if err != nil {
		fmt.Fprintln(os.Stderr, "gen:", err)
		os.Exit(1)
	}
	if err := os.WriteFile(*out, src, 0o644); err != nil {
		fmt.Fprintln(os.Stderr, "gen:", err)
		os.Exit(1)
	}
}
