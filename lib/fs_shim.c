// fs_shim.c -- file read/write helpers for Vit

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

char* vit_file_read(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return "";

    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return "";
    }

    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return "";
    }

    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return "";
    }

    char* buf = (char*)malloc((size_t)size + 1);
    if (!buf) {
        fclose(f);
        return "";
    }

    size_t n = fread(buf, 1, (size_t)size, f);
    fclose(f);
    buf[n] = '\0';
    return buf;
}

int vit_file_write(const char* path, const char* data) {
    FILE* f = fopen(path, "wb");
    if (!f) return -1;

    size_t len = strlen(data);
    size_t n = fwrite(data, 1, len, f);
    fclose(f);
    return n == len ? 0 : -1;
}

int vit_file_append(const char* path, const char* data) {
    FILE* f = fopen(path, "ab");
    if (!f) return -1;

    size_t len = strlen(data);
    size_t n = fwrite(data, 1, len, f);
    fclose(f);
    return n == len ? 0 : -1;
}

int vit_file_exists(const char* path) {
    struct stat st;
    return stat(path, &st) == 0 ? 1 : 0;
}

int vit_file_free(char* s) {
    if (!s || s[0] == '\0') return 0;
    free(s);
    return 0;
}
