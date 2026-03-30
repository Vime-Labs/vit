// http_runtime_shim.c -- robust HTTP request read/write helpers for Vit

#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

#ifndef MSG_NOSIGNAL
#define MSG_NOSIGNAL 0
#endif

static int parse_content_length(const char* buf, int headers_end) {
    const char* p = buf;
    const char* end = buf + headers_end;

    while (p < end) {
        const char* line_end = strstr(p, "\r\n");
        if (!line_end || line_end > end) break;

        if ((line_end - p) >= 15 && strncmp(p, "Content-Length:", 15) == 0) {
            const char* v = p + 15;
            while (v < line_end && (*v == ' ' || *v == '\t')) v++;
            return atoi(v);
        }

        if ((line_end - p) >= 15 && strncmp(p, "content-length:", 15) == 0) {
            const char* v = p + 15;
            while (v < line_end && (*v == ' ' || *v == '\t')) v++;
            return atoi(v);
        }

        p = line_end + 2;
    }

    return -1;
}

char* vit_http_read_request(int fd, int max_bytes) {
    if (max_bytes <= 0) {
        char* empty = (char*)malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }

    int cap = 4096;
    if (cap > max_bytes + 1) cap = max_bytes + 1;
    if (cap < 64) cap = max_bytes + 1;

    char* buf = (char*)malloc((size_t)cap);
    if (!buf) return NULL;

    int len = 0;
    int headers_end = -1;
    int content_length = -1;

    while (len < max_bytes) {
        if (cap - len <= 1) {
            int new_cap = cap * 2;
            if (new_cap > max_bytes + 1) new_cap = max_bytes + 1;
            if (new_cap <= cap) break;

            char* tmp = (char*)realloc(buf, (size_t)new_cap);
            if (!tmp) break;
            buf = tmp;
            cap = new_cap;
        }

        int space = cap - len - 1;
        int remaining = max_bytes - len;
        int want = space < remaining ? space : remaining;
        if (want <= 0) break;

        int n = (int)recv(fd, buf + len, (size_t)want, 0);
        if (n <= 0) break;

        len += n;
        buf[len] = '\0';

        if (headers_end < 0) {
            char* marker = strstr(buf, "\r\n\r\n");
            if (marker) {
                headers_end = (int)(marker - buf) + 4;
                content_length = parse_content_length(buf, headers_end);

                if (content_length < 0) {
                    break;
                }
            }
        }

        if (headers_end >= 0 && content_length >= 0) {
            int total_needed = headers_end + content_length;
            if (len >= total_needed) break;
        }
    }

    buf[len] = '\0';
    return buf;
}

int vit_http_send_all(int fd, const char* data, int len) {
    int sent = 0;

    while (sent < len) {
        int n = (int)send(fd, data + sent, (size_t)(len - sent), MSG_NOSIGNAL);
        if (n <= 0) {
            return sent > 0 ? sent : n;
        }
        sent += n;
    }

    return sent;
}
