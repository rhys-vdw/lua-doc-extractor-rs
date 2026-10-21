use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use lua_doc_extractor::{Options, doc::Severity, extract, scan::scan};
use serde_json::json;

/// Extracts Lua docs from C-style comments.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Source files or glob patterns.
    #[arg(long, num_args = 1..)]
    src: Vec<String>,

    /// Directory to write Lua output to.
    #[arg(long, short, default_value = ".")]
    dest: PathBuf,

    /// Base URL for source links, e.g. https://github.com/org/repo/blob/<sha>/
    #[arg(long)]
    repo: Option<String>,

    /// Exit with an error code if any errors are reported.
    #[arg(long)]
    error: bool,

    /// Write a map from each generated line to its C++ line to this JSON file.
    #[arg(long)]
    sourcemap: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let options = Options { repo: args.repo };

    let mut files: Vec<PathBuf> = Vec::new();
    for pattern in args.src.iter().flat_map(|p| expand_braces(p)) {
        match glob::glob(&pattern) {
            Ok(paths) => files.extend(paths.filter_map(Result::ok)),
            Err(e) => {
                eprintln!("error: invalid pattern '{pattern}': {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        eprintln!("error: no source files");
        return ExitCode::FAILURE;
    }

    let mut error_count = 0;
    let mut sourcemap = BTreeMap::new();
    for file in &files {
        let Some(path) = relative_path(file) else {
            eprintln!(
                "{}: error: source files must be inside the working directory, so that the \
                 output can mirror their paths under --dest",
                file.display()
            );
            error_count += 1;
            continue;
        };
        let source = match std::fs::read_to_string(file) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("{path}: error: {e}");
                error_count += 1;
                continue;
            }
        };
        if scan(&source).is_empty() {
            continue;
        }

        let (rendered, diagnostics) = extract(&path, &source, &options);
        for d in &diagnostics {
            let severity = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            eprintln!(
                "{}:{}:{}: {severity}: {}",
                d.path, d.pos.line, d.pos.col, d.message
            );
        }
        error_count += diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();

        let output = format!("{path}.lua");
        let out = args.dest.join(&output);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).expect("create output directory");
        }
        std::fs::write(&out, &rendered.lua).expect("write output");
        sourcemap.insert(
            output,
            json!({ "source": path, "lines": rendered.line_map }),
        );
    }

    if let Some(file) = &args.sourcemap {
        let json = json!({ "version": 1, "files": sourcemap });
        let text = serde_json::to_string(&json).expect("serialise source map");
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("create source map directory");
        }
        std::fs::write(file, text).expect("write source map");
    }

    if args.error && error_count > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Expands `{a,b}` alternatives, which the `glob` crate doesn't support:
/// `rts/{Lua,Rml}/*.cpp` becomes `rts/Lua/*.cpp` and `rts/Rml/*.cpp`.
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let mut depth = 0;
    let mut parts = Vec::new();
    let mut part_start = open + 1;
    for (i, c) in pattern[open..].char_indices().map(|(i, c)| (i + open, c)) {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => {
                parts.push(&pattern[part_start..i]);
                let (prefix, suffix) = (&pattern[..open], &pattern[i + 1..]);
                return parts
                    .into_iter()
                    .flat_map(|part| expand_braces(&format!("{prefix}{part}{suffix}")))
                    .collect();
            }
            '}' => depth -= 1,
            ',' if depth == 1 => {
                parts.push(&pattern[part_start..i]);
                part_start = i + 1;
            }
            _ => {}
        }
    }
    vec![pattern.to_string()]
}

/// The path relative to the working directory, with forward slashes, or `None` if the file is
/// outside it. Output is written to the same path under `--dest`, so it must stay inside.
fn relative_path(file: &Path) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let relative = if file.is_absolute() {
        file.strip_prefix(&cwd).ok()?
    } else {
        file
    };
    let inside = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    inside.then(|| relative.to_string_lossy().replace('\\', "/"))
}
