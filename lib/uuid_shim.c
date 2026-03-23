// uuid_shim.c — UUID v4 generator for Vit
//
// Uses /dev/urandom for cryptographically random bytes.
// Self-contained: no external libraries required.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

char* vit_uuid_v4(void) {
    uint8_t b[16];

    FILE* f = fopen("/dev/urandom", "rb");
    if (!f) {
        // Fallback: not great but won't crash
        char* s = malloc(37);
        snprintf(s, 37, "00000000-0000-4000-8000-000000000000");
        return s;
    }
    fread(b, 1, 16, f);
    fclose(f);

    // Set version (4) and variant (10xx)
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;

    char* s = malloc(37);
    snprintf(s, 37,
        "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
        b[0],b[1],b[2],b[3], b[4],b[5], b[6],b[7],
        b[8],b[9], b[10],b[11],b[12],b[13],b[14],b[15]);
    return s;
}

void vit_uuid_free(char* s) {
    if (s) free(s);
}
