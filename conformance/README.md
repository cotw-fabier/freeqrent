# Conformance suite

`cases.json` is the suite every freeQRent binding runs (SPEC §10.3). Output is deterministic, so every language has to produce the same bytes.

Each case has a `name`, a function `fn` and its arguments:

| `fn` | Arguments |
|---|---|
| `render`, `check`, `plan` | `payload`: `{"text": …}` or `{"bytes": base64}`; `style`: a Style document |
| `palette_from_css`, `palette_from_html` | `text`, `opts` |
| `palette_from_image` | `image`: `{"width", "height", "rgba": base64}`; `opts` |
| `presets` | none |

A runner serializes `style` and `opts` with its own JSON writer, calls the binding's function, and compares the result with `expect`. That's either `{"sha256", "length"}` of the returned text's UTF-8 bytes, or `{"error": code}` for a call that has to fail with that error code.

The file is generated. Don't edit it by hand:

```sh
cargo run -p freeqrent-doc --example generate
```

The inputs live in `crates/freeqrent-doc/tests/support/mod.rs`. A changed hash means the output changed, so review it like a snapshot and say why in the wave log. The Rust runner (`crates/freeqrent-doc/tests/conformance.rs`) also checks that every document fits `schema/freeqrent.schema.json` and that every rendered code scans.
