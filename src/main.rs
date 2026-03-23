mod lexer;
mod ast;
mod parser;
mod codegen;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  vit [build] <source.vit> [output] [extra_link_flags...]");
    eprintln!("  vit run     <source.vit>           [extra_link_flags...]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -v, --verbose   Show tokens, AST and LLVM IR");
    eprintln!();
    eprintln!("Link flags declared inside .vit files with:  link \"-lfoo\";");
    eprintln!("Extra link flags can also be passed on the CLI as a fallback.");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  vit build server.vit");
    eprintln!("  vit run   hello.vit");
    eprintln!("  vit run   app.vit        # lib flags come from 'link' directives");
    eprintln!("  vit build app.vit myapp  # explicit output name");
}

fn print_aidocs() {
    print!(r#"# Vit Language — AI Reference

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

/// Returns ~/.vit/lib — the stdlib search path.
fn stdlib_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".vit").join("lib")
}

/// Resolve `import "path";` and `link "flag";` directives recursively.
/// - import: inlines the file content (deduped by canonical path)
///           falls back to ~/.vit/lib/<path> when not found locally
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

            // Search order: local path first, then ~/.vit/lib/<path>
            let local_path = base_dir.join(path_str);
            let full_path = if local_path.exists() {
                local_path
            } else {
                let stdlib_path = stdlib_dir().join(path_str);
                if stdlib_path.exists() {
                    stdlib_path
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
