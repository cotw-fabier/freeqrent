/*
 * freeQRent C ABI (SPEC §10.2). Styled and animated SVG QR codes from a
 * Style document (JSON; schema/freeqrent.schema.json).
 *
 * Every call writes one string to *out and returns FREEQRENT_OK (*out is
 * the result) or FREEQRENT_ERROR (*out is the error as JSON:
 * {"code","message","detail"}). If out_len isn't NULL it gets the
 * string's length in bytes, without the terminating NUL. Free every
 * returned string with freeqrent_free. Only a NULL out returns
 * FREEQRENT_INVALID, with nothing written.
 *
 * Strings are UTF-8. A NULL style or options string means none ("{}").
 * The functions are pure and safe to call from any thread at once.
 */
#ifndef FREEQRENT_H
#define FREEQRENT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define FREEQRENT_OK 0
#define FREEQRENT_ERROR 1
#define FREEQRENT_INVALID 2

/* Payload kinds. */
#define FREEQRENT_TEXT 0
#define FREEQRENT_BYTES 1

/* The SVG for len bytes of payload (kind FREEQRENT_TEXT: UTF-8 text;
 * FREEQRENT_BYTES: raw bytes), drawn with the Style document style_json. */
int32_t freeqrent_render(const uint8_t *data, size_t len, int32_t kind,
                         const char *style_json, char **out, size_t *out_len);

/* The intro stopped at t_ms, as a still SVG with no animation (the
 * finished code for a style without an intro). */
int32_t freeqrent_render_frame(const uint8_t *data, size_t len, int32_t kind,
                               const char *style_json, uint32_t t_ms,
                               char **out, size_t *out_len);

/* The scan report as JSON. A style freeqrent_render would refuse still gets
 * a report, with "ok" false. */
int32_t freeqrent_check(const uint8_t *data, size_t len, int32_t kind,
                        const char *style_json, char **out, size_t *out_len);

/* The animation plan as JSON (SPEC §8.5). */
int32_t freeqrent_plan(const uint8_t *data, size_t len, int32_t kind,
                       const char *style_json, char **out, size_t *out_len);

/* A palette from width x height RGBA pixels, row by row, as JSON. */
int32_t freeqrent_palette_from_image(const uint8_t *rgba, size_t rgba_len,
                                     uint32_t width, uint32_t height,
                                     const char *opts_json, char **out,
                                     size_t *out_len);

/* A palette from a style sheet's colors, as JSON. */
int32_t freeqrent_palette_from_css(const char *css, const char *opts_json,
                                   char **out, size_t *out_len);

/* A palette from a page's colors, as JSON. */
int32_t freeqrent_palette_from_html(const char *html, const char *opts_json,
                                    char **out, size_t *out_len);

/* The preset names as a JSON array. */
int32_t freeqrent_presets(char **out, size_t *out_len);

/* The library version, e.g. "1.0.0". Static; don't free it. */
const char *freeqrent_version(void);

/* len bytes of zeroed memory, for a host that can't allocate in this
 * library's memory itself (a WebAssembly host passing a payload in); NULL
 * when len is 0. Free it with freeqrent_dealloc(ptr, len). */
uint8_t *freeqrent_alloc(size_t len);
void freeqrent_dealloc(uint8_t *ptr, size_t len);

/* Frees a string this library returned. NULL is ignored. */
void freeqrent_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* FREEQRENT_H */
