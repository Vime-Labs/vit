// json_parse_shim.c — cJSON wrappers for Vit
//
// cJSON* handles are cast to char* so Vit's str type can hold them as opaque
// pointers — same pattern used by sqlite_shim.c.
//
// cJSON.c and cJSON.h are downloaded into ~/.vit/lib/ by install.sh.
// They are compiled separately via `link "cJSON.c"` in json_parse.vit.

#include "cJSON.h"
#include <stdlib.h>
#include <string.h>

static const char* safe_str(const char* s) {
    return s ? s : "";
}

// ── parse / free ──────────────────────────────────────────────────────────────

// Returns opaque cJSON* handle, or NULL on parse error.
char* vit_json_parse(const char* s) {
    return (char*)cJSON_Parse(s);
}

void vit_json_free(char* j) {
    cJSON_Delete((cJSON*)j);
}

// 1 if the handle is non-null and the parse succeeded, 0 otherwise.
int vit_json_valid(char* j) {
    return j != NULL ? 1 : 0;
}

// ── object field access ───────────────────────────────────────────────────────

int vit_json_has(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    return item ? 1 : 0;
}

const char* vit_json_get_str(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    if (cJSON_IsString(item)) return safe_str(item->valuestring);
    return "";
}

int vit_json_get_int(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    if (cJSON_IsNumber(item)) return (int)item->valuedouble;
    return 0;
}

double vit_json_get_float(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    if (cJSON_IsNumber(item)) return item->valuedouble;
    return 0.0;
}

int vit_json_get_bool(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    if (cJSON_IsBool(item)) return cJSON_IsTrue(item) ? 1 : 0;
    return 0;
}

// Returns a handle to the nested object (NOT a copy — do NOT free separately).
char* vit_json_get_obj(char* j, const char* key) {
    cJSON* item = cJSON_GetObjectItemCaseSensitive((cJSON*)j, key);
    if (cJSON_IsObject(item) || cJSON_IsArray(item)) return (char*)item;
    return NULL;
}

// ── array access ──────────────────────────────────────────────────────────────

int vit_json_arr_len(char* j) {
    return cJSON_GetArraySize((cJSON*)j);
}

// Returns a handle to the element at index i (NOT a copy — do NOT free).
char* vit_json_arr_get(char* j, int i) {
    cJSON* item = cJSON_GetArrayItem((cJSON*)j, i);
    return item ? (char*)item : NULL;
}

const char* vit_json_arr_get_str(char* j, int i) {
    cJSON* item = cJSON_GetArrayItem((cJSON*)j, i);
    if (cJSON_IsString(item)) return safe_str(item->valuestring);
    return "";
}

int vit_json_arr_get_int(char* j, int i) {
    cJSON* item = cJSON_GetArrayItem((cJSON*)j, i);
    if (cJSON_IsNumber(item)) return (int)item->valuedouble;
    return 0;
}

// ── item type checks (useful for dynamic responses) ───────────────────────────

int vit_json_is_null(char* j)   { return cJSON_IsNull((cJSON*)j)   ? 1 : 0; }
int vit_json_is_str(char* j)    { return cJSON_IsString((cJSON*)j)  ? 1 : 0; }
int vit_json_is_int(char* j)    { return cJSON_IsNumber((cJSON*)j)  ? 1 : 0; }
int vit_json_is_bool(char* j)   { return cJSON_IsBool((cJSON*)j)    ? 1 : 0; }
int vit_json_is_obj(char* j)    { return cJSON_IsObject((cJSON*)j)  ? 1 : 0; }
int vit_json_is_arr(char* j)    { return cJSON_IsArray((cJSON*)j)   ? 1 : 0; }

// ── stringify (for debug) ─────────────────────────────────────────────────────

// Caller must free the returned string with vit_json_str_free().
char* vit_json_to_str(char* j) {
    char* s = cJSON_PrintUnformatted((cJSON*)j);
    return s ? s : strdup("");
}

void vit_json_str_free(char* s) {
    if (s) free(s);
}
