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
