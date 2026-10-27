/* A C host calling the library: render, a refused style, a report, the
 * presets and the version. Built with -Wall -Wextra -Werror -pedantic by
 * scripts/ffi-smoke.sh, so the header stays valid C99. */
#include <stdio.h>
#include <string.h>

#include "freeqrent.h"

static int failures = 0;

static void expect(int ok, const char *what) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", what);
        failures++;
    }
}

int main(void) {
    const char *text = "https://example.com/c";
    const char *style = "{\"preset\":\"ink\",\"intro\":\"ripple\"}";
    char *out = NULL;
    size_t len = 0;

    int32_t code = freeqrent_render((const uint8_t *)text, strlen(text), FREEQRENT_TEXT, style, &out, &len);
    expect(code == FREEQRENT_OK, "render works");
    expect(out != NULL && strncmp(out, "<svg", 4) == 0, "render returns SVG");
    expect(out != NULL && strlen(out) == len, "out_len is the length");
    freeqrent_free(out);

    code = freeqrent_render((const uint8_t *)text, strlen(text), FREEQRENT_TEXT, "{\"palette\":{\"fg\":\"#ddd\"}}", &out, NULL);
    expect(code == FREEQRENT_ERROR, "a pale fg is refused");
    expect(out != NULL && strstr(out, "\"code\":\"contrast_too_low\"") != NULL, "the error says why");
    freeqrent_free(out);

    code = freeqrent_check((const uint8_t *)text, strlen(text), FREEQRENT_TEXT, NULL, &out, &len);
    expect(code == FREEQRENT_OK && strncmp(out, "{\"ok\":true", 10) == 0, "check reports");
    freeqrent_free(out);

    code = freeqrent_presets(&out, &len);
    expect(code == FREEQRENT_OK && strstr(out, "\"terminal\"") != NULL, "presets are listed");
    freeqrent_free(out);

    expect(freeqrent_presets(NULL, NULL) == FREEQRENT_INVALID, "a NULL out is invalid");
    expect(strcmp(freeqrent_version(), "1.0.0") == 0, "the version");

    if (failures == 0) {
        printf("freeqrent-ffi: C smoke test passed (version %s)\n", freeqrent_version());
    }
    return failures == 0 ? 0 : 1;
}
