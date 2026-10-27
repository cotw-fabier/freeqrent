# freeqrent-ffi

The C ABI of [freeQRent](https://github.com/freeqrent/freeqrent): styled and animated SVG QR codes from a JSON Style document, for Dart, Go, Swift and any other C host. The header is `include/freeqrent.h`.

```c
char *svg = NULL;
const char *text = "https://example.com";
if (freeqrent_render((const uint8_t *)text, strlen(text), FREEQRENT_TEXT,
                     "{\"preset\":\"ink\",\"intro\":\"ripple\"}", &svg, NULL) == FREEQRENT_OK) {
    puts(svg);
}
freeqrent_free(svg); /* on error, svg holds {"code","message","detail"} */
```

Every call returns `FREEQRENT_OK` with the result, or `FREEQRENT_ERROR` with the error as JSON, in the same `out` string. The document format and the error codes are in `SPEC.md` §10.1, and the schema is in `schema/freeqrent.schema.json`.
