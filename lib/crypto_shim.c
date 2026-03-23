// crypto_shim.c — SHA-256, HMAC-SHA-256 and Base64 for Vit
//
// Self-contained: no OpenSSL or external libs required.
// SHA-256 follows FIPS 180-4. HMAC follows RFC 2104.

#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

// ── SHA-256 ───────────────────────────────────────────────────────────────────

#define ROR32(x,n) (((x) >> (n)) | ((x) << (32-(n))))

static const uint32_t K[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,
    0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,
    0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,
    0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,
    0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,
    0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,
    0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,
    0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,
    0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
};

typedef struct {
    uint32_t state[8];
    uint64_t count;
    uint8_t  buf[64];
} SHA256_CTX;

static void sha256_transform(SHA256_CTX* ctx, const uint8_t* data) {
    uint32_t a,b,c,d,e,f,g,h,t1,t2,w[64];
    int i;
    for (i = 0; i < 16; i++)
        w[i] = ((uint32_t)data[i*4]<<24)|((uint32_t)data[i*4+1]<<16)|
               ((uint32_t)data[i*4+2]<<8)|(uint32_t)data[i*4+3];
    for (i = 16; i < 64; i++) {
        uint32_t s0 = ROR32(w[i-15],7)^ROR32(w[i-15],18)^(w[i-15]>>3);
        uint32_t s1 = ROR32(w[i-2],17)^ROR32(w[i-2],19)^(w[i-2]>>10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a=ctx->state[0]; b=ctx->state[1]; c=ctx->state[2]; d=ctx->state[3];
    e=ctx->state[4]; f=ctx->state[5]; g=ctx->state[6]; h=ctx->state[7];
    for (i = 0; i < 64; i++) {
        t1 = h + (ROR32(e,6)^ROR32(e,11)^ROR32(e,25))
               + ((e&f)^(~e&g)) + K[i] + w[i];
        t2 = (ROR32(a,2)^ROR32(a,13)^ROR32(a,22))
               + ((a&b)^(a&c)^(b&c));
        h=g; g=f; f=e; e=d+t1;
        d=c; c=b; b=a; a=t1+t2;
    }
    ctx->state[0]+=a; ctx->state[1]+=b; ctx->state[2]+=c; ctx->state[3]+=d;
    ctx->state[4]+=e; ctx->state[5]+=f; ctx->state[6]+=g; ctx->state[7]+=h;
}

static void sha256_init(SHA256_CTX* ctx) {
    ctx->state[0]=0x6a09e667; ctx->state[1]=0xbb67ae85;
    ctx->state[2]=0x3c6ef372; ctx->state[3]=0xa54ff53a;
    ctx->state[4]=0x510e527f; ctx->state[5]=0x9b05688c;
    ctx->state[6]=0x1f83d9ab; ctx->state[7]=0x5be0cd19;
    ctx->count=0;
}

static void sha256_update(SHA256_CTX* ctx, const uint8_t* data, size_t len) {
    size_t i, used = ctx->count % 64;
    ctx->count += len;
    if (used) {
        size_t free = 64 - used;
        if (len < free) { memcpy(ctx->buf+used, data, len); return; }
        memcpy(ctx->buf+used, data, free);
        sha256_transform(ctx, ctx->buf);
        data += free; len -= free;
    }
    for (i = 0; i+64 <= len; i+=64) sha256_transform(ctx, data+i);
    memcpy(ctx->buf, data+i, len-i);
}

static void sha256_final(SHA256_CTX* ctx, uint8_t out[32]) {
    size_t used = ctx->count % 64;
    ctx->buf[used++] = 0x80;
    if (used > 56) { memset(ctx->buf+used, 0, 64-used); sha256_transform(ctx, ctx->buf); used=0; }
    memset(ctx->buf+used, 0, 56-used);
    uint64_t bits = ctx->count * 8;
    for (int i = 7; i >= 0; i--) { ctx->buf[56+i] = bits & 0xff; bits >>= 8; }
    sha256_transform(ctx, ctx->buf);
    for (int i = 0; i < 8; i++) {
        out[i*4+0] = (ctx->state[i]>>24)&0xff; out[i*4+1] = (ctx->state[i]>>16)&0xff;
        out[i*4+2] = (ctx->state[i]>>8)&0xff;  out[i*4+3] = ctx->state[i]&0xff;
    }
}

// ── public: sha256(str) -> hex str (64 chars, heap-allocated) ─────────────────

char* vit_sha256(const char* data) {
    SHA256_CTX ctx;
    uint8_t hash[32];
    sha256_init(&ctx);
    sha256_update(&ctx, (const uint8_t*)data, strlen(data));
    sha256_final(&ctx, hash);
    char* hex = malloc(65);
    for (int i = 0; i < 32; i++) sprintf(hex+i*2, "%02x", hash[i]);
    hex[64] = '\0';
    return hex;
}

// ── HMAC-SHA-256 ──────────────────────────────────────────────────────────────

char* vit_hmac_sha256(const char* key, const char* data) {
    uint8_t k[64] = {0};
    size_t klen = strlen(key);

    // If key > 64 bytes, hash it first
    if (klen > 64) {
        SHA256_CTX ctx;
        sha256_init(&ctx);
        sha256_update(&ctx, (const uint8_t*)key, klen);
        sha256_final(&ctx, k);
    } else {
        memcpy(k, key, klen);
    }

    uint8_t ipad[64], opad[64];
    for (int i = 0; i < 64; i++) { ipad[i] = k[i]^0x36; opad[i] = k[i]^0x5c; }

    // inner = SHA256(ipad || data)
    SHA256_CTX ctx;
    uint8_t inner[32];
    sha256_init(&ctx);
    sha256_update(&ctx, ipad, 64);
    sha256_update(&ctx, (const uint8_t*)data, strlen(data));
    sha256_final(&ctx, inner);

    // outer = SHA256(opad || inner)
    uint8_t outer[32];
    sha256_init(&ctx);
    sha256_update(&ctx, opad, 64);
    sha256_update(&ctx, inner, 32);
    sha256_final(&ctx, outer);

    char* hex = malloc(65);
    for (int i = 0; i < 32; i++) sprintf(hex+i*2, "%02x", outer[i]);
    hex[64] = '\0';
    return hex;
}

// ── Base64 ────────────────────────────────────────────────────────────────────

static const char B64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

char* vit_base64_encode(const char* data) {
    size_t len = strlen(data);
    size_t out_len = 4 * ((len + 2) / 3) + 1;
    char* out = malloc(out_len);
    size_t i, j = 0;
    for (i = 0; i+2 < len; i+=3) {
        out[j++] = B64[(uint8_t)data[i]>>2];
        out[j++] = B64[((uint8_t)data[i]&3)<<4 | (uint8_t)data[i+1]>>4];
        out[j++] = B64[((uint8_t)data[i+1]&0xf)<<2 | (uint8_t)data[i+2]>>6];
        out[j++] = B64[(uint8_t)data[i+2]&0x3f];
    }
    if (i < len) {
        out[j++] = B64[(uint8_t)data[i]>>2];
        if (i+1 < len) {
            out[j++] = B64[((uint8_t)data[i]&3)<<4 | (uint8_t)data[i+1]>>4];
            out[j++] = B64[((uint8_t)data[i+1]&0xf)<<2];
        } else {
            out[j++] = B64[((uint8_t)data[i]&3)<<4];
            out[j++] = '=';
        }
        out[j++] = '=';
    }
    out[j] = '\0';
    return out;
}

static int b64val(char c) {
    if (c>='A'&&c<='Z') return c-'A';
    if (c>='a'&&c<='z') return c-'a'+26;
    if (c>='0'&&c<='9') return c-'0'+52;
    if (c=='+') return 62;
    if (c=='/') return 63;
    return -1;
}

char* vit_base64_decode(const char* data) {
    size_t len = strlen(data);
    char* out = malloc(len);
    size_t j = 0;
    for (size_t i = 0; i+3 < len; i+=4) {
        int a=b64val(data[i]), b=b64val(data[i+1]);
        int c=b64val(data[i+2]), d=b64val(data[i+3]);
        if (a<0||b<0) break;
        out[j++] = (char)((a<<2)|(b>>4));
        if (c>=0) out[j++] = (char)((b<<4)|(c>>2));
        if (d>=0) out[j++] = (char)((c<<6)|d);
    }
    out[j] = '\0';
    return out;
}

// ── free heap strings returned by the above functions ─────────────────────────

void vit_crypto_free(char* s) {
    if (s) free(s);
}
