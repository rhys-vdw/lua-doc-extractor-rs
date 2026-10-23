use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use lua_doc_extractor::{
    Options,
    doc::Severity,
    env, extract_library, remap,
    scan::{Syntax, scan},
    source_map,
};

/// Extracts Lua docs from C-style comments.
#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    extract: Extract,
}

#[derive(Subcommand)]
enum Command {
    /// Moves a Lua checker's diagnostics on the library onto the C++ they came from.
    ///
    /// Uses the source map written by --sourcemap. Reads LuaLS's check.json
    /// (lua-language-server --check) or `emmylua_check -f json`. Hints are left out.
    Remap(RemapArgs),
}

#[derive(Args)]
struct RemapArgs {
    /// The checker's JSON output.
    #[arg(required = true)]
    check: Vec<PathBuf>,

    /// The source map written by --sourcemap.
    #[arg(long)]
    sourcemap: PathBuf,

    /// The --dest the library was extracted to.
    #[arg(long, short, default_value = ".")]
    dest: PathBuf,

    /// `github` prints GitHub Actions annotations, which show on a pull request's diff.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Github,
}

#[derive(Args)]
struct Extract {
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

    /// Write a map from each generated line to its C++ file and line to this JSON file.
    #[arg(long)]
    sourcemap: Option<PathBuf>,

    /// How to print warnings and errors. `github` prints GitHub Actions annotations, on standard
    /// output, which show on a pull request's diff.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Write the docs tagged `@env` with exactly these names to this file (relative to
    /// --dest). May be repeated. Docs without a tag go to their source's own file.
    /// e.g. --env synced=synced.lua --env unsynced=unsynced.lua --env synced,unsynced=shared.lua
    #[arg(long, value_name = "NAME[,NAME...]=FILE")]
    env: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Remap(args)) => run_remap(args),
        None => run_extract(cli.extract),
    }
}

fn run_remap(args: RemapArgs) -> ExitCode {
    let read = |path: &Path| -> Result<serde_json::Value, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    };
    let map = match read(&args.sourcemap).and_then(|j| remap::SourceMap::from_json(&j)) {
        Ok(map) => map,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let cwd = std::env::current_dir().expect("working directory");
    for check in &args.check {
        let diagnostics = match read(check).and_then(|j| remap::read_check(&j)) {
            Ok(diagnostics) => diagnostics,
            Err(e) => {
                eprintln!("error: {}: {e}", check.display());
                return ExitCode::FAILURE;
            }
        };
        for d in remap::remap(&diagnostics, &map, &args.dest, &cwd) {
            match args.format {
                Format::Text => println!("{}", remap::text(&d)),
                Format::Github => println!("{}", remap::github(&d)),
            }
        }
    }
    ExitCode::SUCCESS
}

fn run_extract(args: Extract) -> ExitCode {
    let options = Options {
        repo: args.repo,
        syntax: Syntax::default(),
    };
    let (routes, route_errors) = env::parse_routes(&args.env);
    if !route_errors.is_empty() {
        for e in route_errors {
            eprintln!("error: {e}");
        }
        return ExitCode::FAILURE;
    }

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
    let mut sources = Vec::new();
    for file in &files {
        let Some(path) = relative_path(file) else {
            eprintln!(
                "{}: error: source files must be inside the working directory, so that the                  output can mirror their paths under --dest",
                file.display()
            );
            error_count += 1;
            continue;
        };
        match std::fs::read_to_string(file) {
            Ok(source) if scan(&source, &options.syntax).is_empty() => {}
            Ok(source) => sources.push((path, source)),
            Err(e) => {
                eprintln!("{path}: error: {e}");
                error_count += 1;
            }
        }
    }

    let (outputs, diagnostics) = extract_library(&sources, &routes, &options);
    for d in &diagnostics {
        let (severity, level) = match d.severity {
            Severity::Error => ("error", remap::Level::Error),
            Severity::Warning => ("warning", remap::Level::Warning),
        };
        match args.format {
            Format::Text => eprintln!(
                "{}:{}:{}: {severity}: {}",
                d.path, d.pos.line, d.pos.col, d.message
            ),
            Format::Github => println!(
                "{}",
                remap::annotation(
                    level,
                    &d.path,
                    d.pos.line,
                    Some(d.pos.col),
                    Some("lua-doc-extractor"),
                    &d.message
                )
            ),
        }
    }
    error_count += diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();

    for output in &outputs {
        let out = args.dest.join(&output.name);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).expect("create output directory");
        }
        std::fs::write(&out, &output.rendered.lua).expect("write output");
    }

    if let Some(file) = &args.sourcemap {
        let text = serde_json::to_string(&source_map(&outputs)).expect("serialise source map");
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
