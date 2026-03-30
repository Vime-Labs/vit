// sqlite_shim.c — thin C wrappers over SQLite3 API
//
// Simplifies signatures so Vit can call them via extern fn:
//   - sqlite3*  and sqlite3_stmt* are returned directly (cast to char*)
//     instead of using out-parameters (sqlite3**, sqlite3_stmt**)
//   - SQLITE_TRANSIENT used for bind_text so Vit does not need to manage lifetime
//
// Build:
//   clang -c sqlite_shim.c -o sqlite_shim.o
// Link:
//   clang app.o sqlite_shim.o -lsqlite3 -no-pie -o app

#include <sqlite3.h>

char* vit_sqlite_open(const char* filename) {
    sqlite3* db = 0;
    sqlite3_open(filename, &db);
    return (char*)db;
}

int vit_sqlite_close(char* db) {
    return sqlite3_close((sqlite3*)db);
}

int vit_sqlite_exec(char* db, const char* sql) {
    return sqlite3_exec((sqlite3*)db, sql, 0, 0, 0);
}

char* vit_sqlite_prepare(char* db, const char* sql) {
    sqlite3_stmt* stmt = 0;
    sqlite3_prepare_v2((sqlite3*)db, sql, -1, &stmt, 0);
    return (char*)stmt;
}

int vit_sqlite_bind_text(char* stmt, int idx, const char* val) {
    return sqlite3_bind_text((sqlite3_stmt*)stmt, idx, val, -1, SQLITE_TRANSIENT);
}

int vit_sqlite_bind_int(char* stmt, int idx, int val) {
    return sqlite3_bind_int((sqlite3_stmt*)stmt, idx, val);
}

int vit_sqlite_step(char* stmt) {
    return sqlite3_step((sqlite3_stmt*)stmt);
}

const char* vit_sqlite_column_text(char* stmt, int col) {
    return (const char*)sqlite3_column_text((sqlite3_stmt*)stmt, col);
}

int vit_sqlite_column_int(char* stmt, int col) {
    return sqlite3_column_int((sqlite3_stmt*)stmt, col);
}

int vit_sqlite_finalize(char* stmt) {
    return sqlite3_finalize((sqlite3_stmt*)stmt);
}

const char* vit_sqlite_errmsg(char* db) {
    return sqlite3_errmsg((sqlite3*)db);
}
