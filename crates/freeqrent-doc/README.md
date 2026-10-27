# freeqrent-doc

Style documents for [freeQRent](https://github.com/freeqrent/freeqrent): a JSON object that describes a style and how to encode for it. Every language binding goes through this crate, so the same document gives the same SVG bytes in every language.

```rust
use freeqrent_doc::{Payload, render};

let svg = render(
    Payload::Text("https://example.com"),
    r#"{"preset":"ink","modules":{"kind":"dots","size":0.9},"intro":"ripple"}"#,
)?;
# Ok::<(), freeqrent_doc::DocError>(())
```

Errors come back as `{code, message, detail}`: a bad document as `invalid_style` with the JSON path, a style that wouldn't scan as `contrast_too_low`, `logo_too_large` and so on. The document format and the error codes are in `SPEC.md` §10.1. The crate has no dependencies besides `freeqrent`.
