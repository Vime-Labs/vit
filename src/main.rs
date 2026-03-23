mod lexer;
mod ast;
mod parser;
mod codegen;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  vit new   <name> [template]         Create a new project");
    eprintln!("  vit [build] <source.vit> [output]   Compile to binary");
    eprintln!("  vit run     <source.vit>             Compile and run");
    eprintln!();
    eprintln!("Templates for vit new:");
    eprintln!("  api-sqlite    REST API + SQLite  (default)");
    eprintln!("  api-postgres  REST API + PostgreSQL");
    eprintln!("  api           REST API in-memory");
    eprintln!("  cli           Command-line tool");
    eprintln!("  blank         Empty project");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -v, --verbose   Show tokens, AST and LLVM IR");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  vit new myapp                # interactive template selection");
    eprintln!("  vit new myapp api-sqlite     # direct");
    eprintln!("  vit run   main.vit");
    eprintln!("  vit build main.vit myapp");
}

fn print_aidocs() {
    print!("{}", r#"# Vit Language — AI Reference

## Overview
Statically typed, compiled to native binaries via LLVM. C-like syntax. No GC.
Toolchain: `vit build <file.vit>` | `vit run <file.vit>`
Stdlib installed at ~/.vit/lib/ — imported with `import "lib/name.vit";`

## Types
| Type       | Description                        |
|------------|------------------------------------|
| i32        | 32-bit signed integer              |
| i64        | 64-bit signed integer              |
| f64        | 64-bit float (double)              |
| bool       | true / false                       |
| str        | UTF-8 string (C char*)             |
| [T; N]     | Fixed-size array, N known at compile time |
| map[K, V]  | Hash map. Keys: i32/i64/str. Values: i32/i64/str |
| StrBuf     | Dynamic string buffer (built-in)   |
| StructName | User-defined struct                |

## Variables & Assignment
```
let x: i32 = 42;
let s: str;
x = x + 1;
x += 1; x -= 1; x *= 2; x /= 2; x %= 3;
```
Globals: declared outside functions, zero-initialized, literal initializers only.

## Operators
Arithmetic:  + - * / %
Comparison:  == != < > <= >=
Logical:     && || !   (bool only)
Bitwise:     & | ^ << >>
Cast:        x as i64 / x as f64 / x as i32
Precedence (low→high): || → && → == != < > <= >= → + - → * / % & | ^ << >> → - ! (unary) → as

## Control Flow
```
if cond { } else if cond { } else { }
while cond { }
for i in 0..n { }   // i from 0 to n-1, step 1
break;
continue;
return val;
```

## Functions
```
fn name(p1: T1, p2: T2) -> RetType {
    return val;
}
```
- Arrays passed as pointer (no size info)
- Structs passed by pointer (copy on receive)
- Maps passed as pointer — map[K,V] valid as parameter type
- Functions must be declared before use
- Entry point: fn main() -> i32

## Structs
```
struct Point { x: i32, y: i32 }
let p: Point = Point { x: 1, y: 2 };
p.x = 10;
print p.y;
```
Nested structs supported. map[K,V] fields supported. Array fields: not supported.

## Arrays
```
let arr: [i32; 5] = [1, 2, 3, 4, 5];
arr[i] = arr[i] + 1;
for i in 0..5 { print arr[i]; }
```
Fixed size at compile time. No bounds checking. No multidimensional arrays.

## Maps
```
let m: map[str, i32];
map_set(m, "key", 42);
let v: i32 = map_get(m, "key");   // 0 if missing
if map_has(m, "key") { ... }
```
Capacity: 4096 entries. No iteration. No removal. Globals and parameters supported.

## StrBuf (dynamic string)
```
let buf: StrBuf = strbuf_new();
strbuf_append(buf, "hello");
strbuf_append(buf, format(" %d", n));
let s: str = strbuf_to_str(buf);
let n: i32 = strbuf_len(buf);
```

## Built-ins
### I/O
print val;                    // print with newline, multiple: print a, " ", b;
input x: i32;                 // read from stdin
input arr[i];                 // read into array element

### String
format(fmt, ...) -> str       // printf-style: %d %ld %f %s %.2f
add(s1, s2) -> str            // concatenate
len(s) -> i32                 // strlen
substr(s, start, len) -> str  // substring
str_pos(s, sub) -> i32        // index of sub, -1 if not found
strcmp(a, b) -> i32           // 0 if equal
split(s, sep, arr) -> i32     // fills arr, returns count
replace(s, old, new) -> str
remove(s, sub) -> str
str_to_int(s) -> i32
str_to_float(s) -> f64
int_to_str(n) -> str

### Math
abs(x)  min(a,b)  max(a,b)  sqrt(x) -> f64  pow(b,e) -> f64

### Array
sort(arr, n)                  // in-place ascending qsort: i32/i64/f64
len(arr) -> i32               // compile-time size

## Modules
```
import "lib/name.vit";        // relative path; falls back to ~/.vit/lib/
link "-lfoo";                 // linker flag, inherited by importers
link "shim.c";                // C file compiled automatically before link
extern fn name(p: T) -> T;   // declare any C function
```

## Stdlib

### lib/http.vit
Structs: Request { method: str, path: str, body: str, headers: map[str,str] }
Parsing:        http_parse(buf) -> Request
Routing:        http_is(req, method, path) -> i32
                http_starts_with(req, method, prefix) -> i32
                http_path_clean(req) -> str
Headers:        http_header(req, name) -> str
Form:           form_get(body, key) -> str | form_has(body, key) -> i32
Query string:   query_get(req, key) -> str | query_has(req, key) -> i32 | query_str(req) -> str
Server:         http_handle(method, path, fn) | http_listen(port)
Responses:      http_ok(body) http_json(body) http_created(body) http_json_created(body)
                http_no_content() http_bad_request(msg) http_unauthorized(msg)
                http_forbidden(msg) http_not_found() http_unprocessable(msg) http_error(msg)

### lib/json.vit
Object:  json_new() -> StrBuf
         json_str(j,k,v) json_int(j,k,v) json_bool(j,k,v) json_null(j,k) json_raw(j,k,v)
         json_build(j) -> str
Array:   json_arr_new() -> StrBuf
         json_arr_str(a,v) json_arr_int(a,v) json_arr_obj(a,v)
         json_arr_build(a) -> str

### lib/sqlite.vit
Constants: SQLITE_OK=0 SQLITE_ROW=100 SQLITE_DONE=101
sqlite_open(filename) -> str    sqlite_close(db) -> i32
sqlite_exec(db, sql) -> i32     sqlite_prepare(db, sql) -> str
sqlite_bind(stmt, idx, val)     sqlite_step(stmt) -> i32
sqlite_col_text(stmt, col) -> str  sqlite_col_int(stmt, col) -> i32
sqlite_finalize(stmt) -> i32    sqlite_errmsg(db) -> str
Requires: sudo apt install libsqlite3-dev

### lib/env.vit
env_get(name) -> str | env_or(name, default) -> str | env_has(name) -> i32

### lib/net.vit
tcp_listen(port) -> i32 (fd)    tcp_accept(server_fd) -> i32
tcp_read(fd, buf, size) -> i32  tcp_write(fd, data, len) -> i32
tcp_close(fd) -> i32

## Known Limitations
- No type inference (annotations required)
- No generics / templates
- No closures or first-class functions (except as http_handle callbacks)
- No dynamic arrays (Vec) — use StrBuf + SQLite for dynamic data
- No map iteration or removal
- No array struct fields
- for loop: step always 1 (use while for other steps)
- format() buffer: 4096 bytes max (use StrBuf for larger strings)
- Globals: literal initializers only, no expressions
"#);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    // Handle `vit new` before the rest of arg parsing
    if args[1] == "new" {
        cmd_new(&args[2..]);
        process::exit(0);
    }

    let mut verbose     = false;
    let mut do_run      = false;
    let mut positional: Vec<String> = Vec::new();
    let mut cli_link:   Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help"   => { print_usage();   process::exit(0); }
            "--aidocs"        => { print_aidocs();  process::exit(0); }
            "-v" | "--verbose"                => verbose = true,
            "run"   if positional.is_empty()  => do_run = true,
            "build" if positional.is_empty()  => {}
            s if s.starts_with('-')           => cli_link.push(args[i].clone()),
            s if s.ends_with(".c") || s.ends_with(".o") => cli_link.push(args[i].clone()),
            _                                 => positional.push(args[i].clone()),
        }
        i += 1;
    }

    if positional.is_empty() {
        print_usage();
        process::exit(1);
    }

    let source_file = &positional[0];
    let source_path = PathBuf::from(source_file);
    let base_dir    = source_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem        = source_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();

    let tmp_prefix = format!("/tmp/vit_{}", stem);

    let exe_path = if do_run {
        format!("/tmp/vit_{}_bin", stem)
    } else if positional.len() > 1 {
        positional[1].clone()
    } else {
        stem.clone()
    };

    let source = fs::read_to_string(source_file).unwrap_or_else(|err| {
        eprintln!("error: cannot read '{}': {}", source_file, err);
        process::exit(1);
    });

    let mut seen      = HashSet::new();
    let mut src_link: Vec<String> = Vec::new();
    seen.insert(fs::canonicalize(source_file).unwrap_or(source_path.clone()));
    let full_source = resolve_imports(&source, &base_dir, &mut seen, &mut src_link);

    // Flags from source take priority; CLI flags are appended (for overrides)
    src_link.extend(cli_link);
    let link_flags = compile_c_sources(src_link, &tmp_prefix);

    compile(&full_source, &stem, &tmp_prefix, &exe_path, &link_flags, verbose);

    if do_run {
        let status = process::Command::new(&exe_path)
            .status()
            .unwrap_or_else(|e| {
                eprintln!("error: failed to run '{}': {}", exe_path, e);
                process::exit(1);
            });
        process::exit(status.code().unwrap_or(0));
    }
}

// ── vit new ───────────────────────────────────────────────────────────────────

fn cmd_new(args: &[String]) {
    let name = if !args.is_empty() {
        args[0].clone()
    } else {
        print!("Project name: ");
        io::stdout().flush().unwrap();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).unwrap();
        let n = buf.trim().to_string();
        if n.is_empty() {
            eprintln!("error: project name cannot be empty");
            process::exit(1);
        }
        n
    };

    let template = if args.len() >= 2 {
        args[1].clone()
    } else {
        select_template()
    };

    let dir = PathBuf::from(&name);
    if dir.exists() {
        eprintln!("error: directory '{}' already exists", name);
        process::exit(1);
    }
    fs::create_dir_all(&dir).unwrap_or_else(|e| {
        eprintln!("error: cannot create directory '{}': {}", name, e);
        process::exit(1);
    });

    let files = template_files(&name, &template);
    if files.is_empty() {
        eprintln!("error: unknown template '{}'. Options: blank, cli, api, api-sqlite, api-postgres", template);
        process::exit(1);
    }

    for (filename, content) in &files {
        let path = dir.join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&path, content).unwrap_or_else(|e| {
            eprintln!("error: cannot write '{}': {}", path.display(), e);
            process::exit(1);
        });
    }

    eprintln!("\x1b[32m[vit]\x1b[0m Created project '{}'  (template: {})", name, template);
    eprintln!();
    for (f, _) in &files {
        eprintln!("  {}/{}", name, f);
    }
    eprintln!();
    eprintln!("\x1b[36m[vit]\x1b[0m Next steps:");
    eprintln!("  cd {}", name);
    match template.as_str() {
        "blank" | "cli" => eprintln!("  vit run main.vit"),
        _ => {
            eprintln!("  cp .env.example .env");
            eprintln!("  vit run main.vit");
        }
    }
}

fn select_template() -> String {
    let options = [
        ("api-sqlite",   "REST API + SQLite          (recommended)"),
        ("api-postgres", "REST API + PostgreSQL"),
        ("api",          "REST API in-memory"),
        ("cli",          "Command-line tool"),
        ("blank",        "Empty project"),
    ];

    eprintln!("Select a template:");
    for (i, (_, desc)) in options.iter().enumerate() {
        eprintln!("  {}. {}", i + 1, desc);
    }
    print!("> ");
    io::stdout().flush().unwrap();

    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap();
    let choice: usize = buf.trim().parse().unwrap_or(0);
    if choice >= 1 && choice <= options.len() {
        options[choice - 1].0.to_string()
    } else {
        eprintln!("invalid choice, defaulting to api-sqlite");
        "api-sqlite".to_string()
    }
}

fn template_files(name: &str, template: &str) -> Vec<(String, String)> {
    match template {
        "blank"        => tpl_blank(name),
        "cli"          => tpl_cli(name),
        "api"          => tpl_api(name),
        "api-sqlite"   => tpl_api_sqlite(name),
        "api-postgres" => tpl_api_postgres(name),
        _              => vec![],
    }
}

fn makefile(name: &str) -> String {
    format!(
        "APP = {name}\n\
         \n\
         run:\n\
         \tvit run main.vit\n\
         \n\
         build:\n\
         \tvit build main.vit $(APP)\n\
         \n\
         clean:\n\
         \trm -f $(APP) *.o\n\
         \n\
         .PHONY: run build clean\n"
    )
}

// ── blank ─────────────────────────────────────────────────────────────────────

fn tpl_blank(name: &str) -> Vec<(String, String)> {
    vec![
        ("main.vit".into(), format!(
            "// {name}\n\
             \n\
             fn main() -> i32 {{\n\
             \tprint \"hello from {name}\";\n\
             \treturn 0;\n\
             }}\n"
        )),
        ("Makefile".into(), makefile(name)),
    ]
}

// ── cli ───────────────────────────────────────────────────────────────────────

fn tpl_cli(name: &str) -> Vec<(String, String)> {
    vec![
        ("main.vit".into(), format!(
            "// {name} — command-line tool\n\
             //\n\
             // Run:   vit run main.vit\n\
             // Build: vit build main.vit {name}\n\
             \n\
             fn print_usage() -> i32 {{\n\
             \tprint \"Usage: {name} <args>\";\n\
             \treturn 0;\n\
             }}\n\
             \n\
             fn main() -> i32 {{\n\
             \tprint \"hello from {name}\";\n\
             \treturn 0;\n\
             }}\n"
        )),
        ("Makefile".into(), makefile(name)),
    ]
}

// ── api (in-memory) ───────────────────────────────────────────────────────────

fn tpl_api(name: &str) -> Vec<(String, String)> {
    let main_vit = format!(
        "// {name} — REST API (in-memory)\n\
         //\n\
         // Run:   vit run main.vit\n\
         // Build: vit build main.vit {name}\n\
         \n\
         import \"lib/http.vit\";\n\
         import \"lib/json.vit\";\n\
         import \"lib/env.vit\";\n\
         import \"handlers.vit\";\n\
         \n\
         fn main() -> i32 {{\n\
         \thttp_handle(\"GET\",  \"/health\", handle_health);\n\
         \thttp_handle(\"GET\",  \"/items\",  handle_list_items);\n\
         \thttp_handle(\"POST\", \"/items\",  handle_create_item);\n\
         \thttp_handle(\"GET\",  \"/items/\", handle_get_item);\n\
         \n\
         \tlet port: i32 = str_to_int(env_or(\"PORT\", \"8080\"));\n\
         \tprint \"listening on :\", port;\n\
         \thttp_listen(port);\n\
         \treturn 0;\n\
         }}\n"
    );

    let handlers_vit = format!(
        "// handlers.vit — request handlers for {name}\n\
         import \"lib/http.vit\";\n\
         import \"lib/json.vit\";\n\
         \n\
         // In-memory store: items[\"1\"] = name, up to 4096 entries\n\
         let items:  map[str, str];\n\
         let count:  i32 = 0;\n\
         \n\
         fn handle_health(req: Request) -> str {{\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_str(j, \"status\", \"ok\");\n\
         \treturn http_json(json_build(j));\n\
         }}\n\
         \n\
         fn handle_list_items(req: Request) -> str {{\n\
         \tlet arr: StrBuf = json_arr_new();\n\
         \tlet i: i32 = 1;\n\
         \twhile i <= count {{\n\
         \t\tlet sid: str = format(\"%d\", i);\n\
         \t\tif map_has(items, sid) {{\n\
         \t\t\tlet obj: StrBuf = json_new();\n\
         \t\t\tjson_str(obj, \"id\",   sid);\n\
         \t\t\tjson_str(obj, \"name\", map_get(items, sid));\n\
         \t\t\tjson_arr_obj(arr, json_build(obj));\n\
         \t\t}}\n\
         \t\ti = i + 1;\n\
         \t}}\n\
         \treturn http_json(json_arr_build(arr));\n\
         }}\n\
         \n\
         fn handle_get_item(req: Request) -> str {{\n\
         \tlet id: str = substr(req.path, 7, 64);\n\
         \tif map_has(items, id) {{\n\
         \t\tlet obj: StrBuf = json_new();\n\
         \t\tjson_str(obj, \"id\",   id);\n\
         \t\tjson_str(obj, \"name\", map_get(items, id));\n\
         \t\treturn http_json(json_build(obj));\n\
         \t}}\n\
         \treturn http_not_found();\n\
         }}\n\
         \n\
         fn handle_create_item(req: Request) -> str {{\n\
         \tlet name: str = form_get(req.body, \"name\");\n\
         \tif strcmp(name, \"\") == 0 {{\n\
         \t\treturn http_bad_request(\"Missing field: name\");\n\
         \t}}\n\
         \tcount = count + 1;\n\
         \tlet id: str = format(\"%d\", count);\n\
         \tmap_set(items, id, name);\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_bool(j, \"created\", 1);\n\
         \tjson_str(j, \"id\", id);\n\
         \treturn http_json_created(json_build(j));\n\
         }}\n"
    );

    vec![
        ("main.vit".into(),     main_vit),
        ("handlers.vit".into(), handlers_vit),
        ("Makefile".into(),     makefile(name)),
        (".env.example".into(), "PORT=8080\n".into()),
    ]
}

// ── api-sqlite ────────────────────────────────────────────────────────────────

fn tpl_api_sqlite(name: &str) -> Vec<(String, String)> {
    let main_vit = format!(
        "// {name} — REST API + SQLite\n\
         //\n\
         // Run:   vit run main.vit\n\
         // Build: vit build main.vit {name}\n\
         \n\
         import \"lib/http.vit\";\n\
         import \"lib/env.vit\";\n\
         import \"db.vit\";\n\
         import \"handlers.vit\";\n\
         \n\
         fn main() -> i32 {{\n\
         \tdb_init();\n\
         \n\
         \thttp_handle(\"GET\",  \"/health\", handle_health);\n\
         \thttp_handle(\"GET\",  \"/items\",  handle_list_items);\n\
         \thttp_handle(\"POST\", \"/items\",  handle_create_item);\n\
         \thttp_handle(\"GET\",  \"/items/\", handle_get_item);\n\
         \n\
         \tlet port: i32 = str_to_int(env_or(\"PORT\", \"8080\"));\n\
         \tprint \"listening on :\", port;\n\
         \thttp_listen(port);\n\
         \treturn 0;\n\
         }}\n"
    );

    let db_vit = format!(
        "// db.vit — database initialization and queries for {name}\n\
         import \"lib/sqlite.vit\";\n\
         import \"lib/env.vit\";\n\
         \n\
         let db: str;\n\
         \n\
         fn db_init() -> i32 {{\n\
         \tlet path: str = env_or(\"DATABASE_URL\", \"{name}.db\");\n\
         \tdb = sqlite_open(path);\n\
         \tsqlite_exec(db, \"CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0)\");\n\
         \treturn 0;\n\
         }}\n"
    );

    let handlers_vit = format!(
        "// handlers.vit — request handlers for {name}\n\
         // Note: `db` is declared in db.vit, imported before this file.\n\
         import \"lib/http.vit\";\n\
         import \"lib/json.vit\";\n\
         import \"lib/uuid.vit\";\n\
         import \"lib/time.vit\";\n\
         \n\
         fn handle_health(req: Request) -> str {{\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_str(j, \"status\", \"ok\");\n\
         \treturn http_json(json_build(j));\n\
         }}\n\
         \n\
         fn handle_list_items(req: Request) -> str {{\n\
         \tlet limit: i32  = str_to_int(query_get(req, \"limit\"));\n\
         \tlet offset: i32 = str_to_int(query_get(req, \"offset\"));\n\
         \tif limit <= 0 {{\n\
         \t\tlimit = 20;\n\
         \t}}\n\
         \tlet sql: str = format(\"SELECT id, name, created_at FROM items ORDER BY rowid LIMIT %d OFFSET %d\", limit, offset);\n\
         \tlet stmt: str = sqlite_prepare(db, sql);\n\
         \tlet arr: StrBuf = json_arr_new();\n\
         \tlet rc: i32 = sqlite_step(stmt);\n\
         \twhile rc == SQLITE_ROW {{\n\
         \t\tlet obj: StrBuf = json_new();\n\
         \t\tjson_str(obj, \"id\",   sqlite_col_text(stmt, 0));\n\
         \t\tjson_str(obj, \"name\", sqlite_col_text(stmt, 1));\n\
         \t\tjson_int(obj, \"created_at\", sqlite_col_int(stmt, 2));\n\
         \t\tjson_arr_obj(arr, json_build(obj));\n\
         \t\trc = sqlite_step(stmt);\n\
         \t}}\n\
         \tsqlite_finalize(stmt);\n\
         \treturn http_json(json_arr_build(arr));\n\
         }}\n\
         \n\
         fn handle_get_item(req: Request) -> str {{\n\
         \tlet id: str   = substr(req.path, 7, 64);\n\
         \tlet stmt: str = sqlite_prepare(db, \"SELECT id, name, created_at FROM items WHERE id = ?1\");\n\
         \tsqlite_bind(stmt, 1, id);\n\
         \tlet rc: i32 = sqlite_step(stmt);\n\
         \tif rc == SQLITE_ROW {{\n\
         \t\tlet obj: StrBuf = json_new();\n\
         \t\tjson_str(obj, \"id\",   sqlite_col_text(stmt, 0));\n\
         \t\tjson_str(obj, \"name\", sqlite_col_text(stmt, 1));\n\
         \t\tjson_int(obj, \"created_at\", sqlite_col_int(stmt, 2));\n\
         \t\tsqlite_finalize(stmt);\n\
         \t\treturn http_json(json_build(obj));\n\
         \t}}\n\
         \tsqlite_finalize(stmt);\n\
         \treturn http_not_found();\n\
         }}\n\
         \n\
         fn handle_create_item(req: Request) -> str {{\n\
         \tlet name: str = form_get(req.body, \"name\");\n\
         \tif strcmp(name, \"\") == 0 {{\n\
         \t\treturn http_bad_request(\"Missing field: name\");\n\
         \t}}\n\
         \tlet id: str  = uuid_v4();\n\
         \tlet now: i64 = time_now();\n\
         \tlet stmt: str = sqlite_prepare(db, \"INSERT INTO items (id, name, created_at) VALUES (?1, ?2, ?3)\");\n\
         \tsqlite_bind(stmt, 1, id);\n\
         \tsqlite_bind(stmt, 2, name);\n\
         \tsqlite_bind(stmt, 3, format(\"%ld\", now));\n\
         \tsqlite_step(stmt);\n\
         \tsqlite_finalize(stmt);\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_bool(j, \"created\", 1);\n\
         \tjson_str(j, \"id\", id);\n\
         \treturn http_json_created(json_build(j));\n\
         }}\n"
    );

    vec![
        ("main.vit".into(),     main_vit),
        ("db.vit".into(),       db_vit),
        ("handlers.vit".into(), handlers_vit),
        ("Makefile".into(),     makefile(name)),
        (".env.example".into(), format!("PORT=8080\nDATABASE_URL={name}.db\n")),
    ]
}

// ── api-postgres ──────────────────────────────────────────────────────────────

fn tpl_api_postgres(name: &str) -> Vec<(String, String)> {
    let main_vit = format!(
        "// {name} — REST API + PostgreSQL\n\
         //\n\
         // Run:   vit run main.vit\n\
         // Build: vit build main.vit {name}\n\
         \n\
         import \"lib/http.vit\";\n\
         import \"lib/env.vit\";\n\
         import \"db.vit\";\n\
         import \"handlers.vit\";\n\
         \n\
         fn main() -> i32 {{\n\
         \tif db_init() != 0 {{\n\
         \t\treturn 1;\n\
         \t}}\n\
         \n\
         \thttp_handle(\"GET\",  \"/health\", handle_health);\n\
         \thttp_handle(\"GET\",  \"/items\",  handle_list_items);\n\
         \thttp_handle(\"POST\", \"/items\",  handle_create_item);\n\
         \thttp_handle(\"GET\",  \"/items/\", handle_get_item);\n\
         \n\
         \tlet port: i32 = str_to_int(env_or(\"PORT\", \"8080\"));\n\
         \tprint \"listening on :\", port;\n\
         \thttp_listen(port);\n\
         \treturn 0;\n\
         }}\n"
    );

    let db_vit = format!(
        "// db.vit — database initialization and schema for {name}\n\
         import \"lib/postgres.vit\";\n\
         import \"lib/env.vit\";\n\
         \n\
         let db: str;\n\
         \n\
         fn db_init() -> i32 {{\n\
         \tlet url: str = env_or(\"DATABASE_URL\", \"postgresql://postgres:pass@localhost/{name}\");\n\
         \tdb = postgres_connect(url);\n\
         \tif postgres_ok(db) == 0 {{\n\
         \t\tprint \"error: \", postgres_errmsg(db);\n\
         \t\treturn 1;\n\
         \t}}\n\
         \tpostgres_exec(db, \"CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at BIGINT NOT NULL DEFAULT 0)\");\n\
         \treturn 0;\n\
         }}\n"
    );

    let handlers_vit = format!(
        "// handlers.vit — request handlers for {name}\n\
         // Note: `db` is declared in db.vit, imported before this file.\n\
         import \"lib/http.vit\";\n\
         import \"lib/json.vit\";\n\
         import \"lib/uuid.vit\";\n\
         import \"lib/time.vit\";\n\
         \n\
         fn handle_health(req: Request) -> str {{\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_str(j, \"status\", \"ok\");\n\
         \treturn http_json(json_build(j));\n\
         }}\n\
         \n\
         fn handle_list_items(req: Request) -> str {{\n\
         \tlet limit: i32  = str_to_int(query_get(req, \"limit\"));\n\
         \tlet offset: i32 = str_to_int(query_get(req, \"offset\"));\n\
         \tif limit <= 0 {{\n\
         \t\tlimit = 20;\n\
         \t}}\n\
         \tlet sql: str = format(\"SELECT id, name, created_at FROM items ORDER BY rowid LIMIT %d OFFSET %d\", limit, offset);\n\
         \tlet res: str = postgres_query(db, sql);\n\
         \tlet arr: StrBuf = json_arr_new();\n\
         \tlet n: i32 = postgres_nrows(res);\n\
         \tlet i: i32 = 0;\n\
         \twhile i < n {{\n\
         \t\tlet obj: StrBuf = json_new();\n\
         \t\tjson_str(obj, \"id\",   postgres_col(res, i, 0));\n\
         \t\tjson_str(obj, \"name\", postgres_col(res, i, 1));\n\
         \t\tjson_str(obj, \"created_at\", postgres_col(res, i, 2));\n\
         \t\tjson_arr_obj(arr, json_build(obj));\n\
         \t\ti = i + 1;\n\
         \t}}\n\
         \tpostgres_free(res);\n\
         \treturn http_json(json_arr_build(arr));\n\
         }}\n\
         \n\
         fn handle_get_item(req: Request) -> str {{\n\
         \tlet id: str = substr(req.path, 7, 64);\n\
         \tpostgres_param(id);\n\
         \tlet res: str = postgres_query_p(db, \"SELECT id, name, created_at FROM items WHERE id = $1\", 1);\n\
         \tif postgres_nrows(res) == 0 {{\n\
         \t\tpostgres_free(res);\n\
         \t\treturn http_not_found();\n\
         \t}}\n\
         \tlet obj: StrBuf = json_new();\n\
         \tjson_str(obj, \"id\",   postgres_col(res, 0, 0));\n\
         \tjson_str(obj, \"name\", postgres_col(res, 0, 1));\n\
         \tjson_str(obj, \"created_at\", postgres_col(res, 0, 2));\n\
         \tpostgres_free(res);\n\
         \treturn http_json(json_build(obj));\n\
         }}\n\
         \n\
         fn handle_create_item(req: Request) -> str {{\n\
         \tlet name: str = form_get(req.body, \"name\");\n\
         \tif strcmp(name, \"\") == 0 {{\n\
         \t\treturn http_bad_request(\"Missing field: name\");\n\
         \t}}\n\
         \tlet id: str  = uuid_v4();\n\
         \tlet now: i64 = time_now();\n\
         \tpostgres_param(id);\n\
         \tpostgres_param(name);\n\
         \tpostgres_param(format(\"%ld\", now));\n\
         \tpostgres_exec_p(db, \"INSERT INTO items (id, name, created_at) VALUES ($1, $2, $3)\", 3);\n\
         \tlet j: StrBuf = json_new();\n\
         \tjson_bool(j, \"created\", 1);\n\
         \tjson_str(j, \"id\", id);\n\
         \treturn http_json_created(json_build(j));\n\
         }}\n"
    );

    vec![
        ("main.vit".into(),     main_vit),
        ("db.vit".into(),       db_vit),
        ("handlers.vit".into(), handlers_vit),
        ("Makefile".into(),     makefile(name)),
        (".env.example".into(), format!(
            "PORT=8080\nDATABASE_URL=postgresql://postgres:pass@localhost/{name}\n"
        )),
    ]
}

/// Returns ~/.vit/lib — the stdlib search path.
fn stdlib_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".vit").join("lib")
}

/// Resolve `import "path";` and `link "flag";` directives recursively.
/// - import: inlines the file content (deduped by canonical path)
///           falls back to ~/.vit/lib/<path> (or ~/.vit/lib/<path without lib/>)
/// - link: appends the flag to `link_flags` (deduped)
fn resolve_imports(
    source: &str,
    base_dir: &Path,
    seen: &mut HashSet<PathBuf>,
    link_flags: &mut Vec<String>,
) -> String {
    let mut result = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("import ") {
            let path_str = rest.trim().trim_end_matches(';').trim().trim_matches('"');

            // Search order: local path first, then stdlib candidates.
            // `import "lib/http.vit"` should resolve to ~/.vit/lib/http.vit.
            let local_path = base_dir.join(path_str);
            let full_path = if local_path.exists() {
                local_path
            } else {
                let stdlib = stdlib_dir();
                let stdlib_path_direct = stdlib.join(path_str);
                let stdlib_path_without_lib = path_str
                    .strip_prefix("lib/")
                    .map(|p| stdlib.join(p));

                if let Some(path) = stdlib_path_without_lib.as_ref().filter(|p| p.exists()) {
                    path.clone()
                } else if stdlib_path_direct.exists() {
                    stdlib_path_direct
                } else {
                    local_path // let the error below report the original path
                }
            };

            let canonical = fs::canonicalize(&full_path).unwrap_or(full_path.clone());

            if seen.insert(canonical) {
                let imported = fs::read_to_string(&full_path).unwrap_or_else(|e| {
                    eprintln!("error: cannot import '{}': {}", full_path.display(), e);
                    eprintln!("hint: stdlib not found — run the install script or copy lib/ to ~/.vit/lib/");
                    process::exit(1);
                });
                let import_dir = full_path.parent().unwrap_or(Path::new(".")).to_path_buf();
                result.push_str(&resolve_imports(&imported, &import_dir, seen, link_flags));
            }
        } else if let Some(rest) = trimmed.strip_prefix("link ") {
            let flag = rest.trim().trim_end_matches(';').trim().trim_matches('"').to_string();
            // .c paths are resolved relative to the declaring .vit file's directory
            let resolved = if flag.ends_with(".c") {
                let c_path = base_dir.join(&flag);
                fs::canonicalize(&c_path)
                    .unwrap_or(c_path)
                    .to_string_lossy()
                    .to_string()
            } else {
                flag
            };
            if !link_flags.contains(&resolved) {
                link_flags.push(resolved);
            }
            // line consumed — not passed to the lexer
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// For each `.c` entry in `flags`, compile it to a temp `.o` and replace the entry.
/// All other flags are passed through unchanged.
fn compile_c_sources(flags: Vec<String>, tmp_prefix: &str) -> Vec<String> {
    flags.into_iter().map(|flag| {
        if !flag.ends_with(".c") {
            return flag;
        }
        let stem = Path::new(&flag)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("shim");
        let obj_path = format!("{}_{}.o", tmp_prefix, stem);
        let status = process::Command::new("clang")
            .args(["-c", &flag, "-o", &obj_path])
            .status()
            .unwrap_or_else(|e| {
                eprintln!("error: failed to run clang for '{}': {}", flag, e);
                process::exit(1);
            });
        if !status.success() {
            eprintln!("error: clang failed to compile '{}'", flag);
            process::exit(1);
        }
        obj_path
    }).collect()
}

fn compile(
    source: &str,
    module_name: &str,
    tmp_prefix: &str,
    exe_path: &str,
    link_flags: &[String],
    verbose: bool,
) {
    let tokens = lexer::tokenize(source);
    if verbose {
        eprintln!("=== Tokens ===");
        for token in &tokens { eprintln!("{:?}", token); }
        eprintln!();
    }

    let program = parser::parse(tokens).unwrap_or_else(|err| {
        eprintln!("error: {}", err);
        process::exit(1);
    });
    if verbose {
        eprintln!("=== AST ===");
        eprintln!("{}", program);
        eprintln!();
    }

    codegen::generate(&program, module_name, tmp_prefix, exe_path, link_flags, verbose)
        .unwrap_or_else(|err| {
            eprintln!("error: {}", err);
            process::exit(1);
        });
}
