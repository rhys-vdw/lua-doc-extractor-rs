//! Generates an emmylua library from Lua doc comments in C++ source.
//!
//! ```text
//! C++ ─scan→ Block ─parse→ Doc ─route→ output ─interpret→ Decl ─resolve→ Item ─emit→ Lua
//! ```
//!
//! Each stage is a function from one value to the next. A `Doc` is what was written, a
//! `Decl` is what one doc declares, and an `Item` is a declaration placed in the library.
//! Routing decides which output file each doc goes to: its source's own, or one named by
//! `--env` for the environments in its `@env`.

pub mod decl;
pub mod doc;
pub mod emit;
pub mod env;
pub mod library;
pub mod name;
pub mod remap;
pub mod scan;
mod text;
pub mod types;

use std::collections::{HashMap, HashSet};

use doc::{Diagnostic, Doc, Severity, parse_doc};
pub use emit::{Options, Rendered};
use env::Routes;
use serde_json::{Value, json};
pub use text::Src;

/// One generated Lua file.
#[derive(Debug, Clone)]
pub struct Output {
    /// The file's path, relative to the output directory.
    pub name: String,
    /// The source files its docs came from, in order.
    pub sources: Vec<String>,
    pub rendered: Rendered,
}

/// Extracts one source file into one Lua file, with its line map.
pub fn extract(path: &str, source: &str, options: &Options) -> (Rendered, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut docs = parse_file(path, source, options, &mut diagnostics);
    env::apply_file_env(&mut docs, &mut diagnostics);
    let rendered = build(docs, options, &mut diagnostics);
    sort(&mut diagnostics);
    (rendered, diagnostics)
}

/// Extracts source files, given as `(path, source)`, into a library. Each doc goes to its
/// source's own output (`<path>.lua`), unless `routes` names an output for its environments.
pub fn extract_library(
    files: &[(String, String)],
    routes: &Routes,
    options: &Options,
) -> (Vec<Output>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    // Each output's docs, in the order the outputs are first used.
    let mut outputs: Vec<(String, Vec<Doc>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for (path, source) in files {
        let mut docs = parse_file(path, source, options, &mut diagnostics);
        env::apply_file_env(&mut docs, &mut diagnostics);
        let mut unrouted = HashSet::new();
        for doc in docs {
            let envs = if routes.is_empty() {
                vec![]
            } else {
                doc.envs()
            };
            let name = if envs.is_empty() {
                format!("{path}.lua")
            } else if let Some(file) = routes.get(&envs) {
                file.to_string()
            } else {
                // One error per file and set of environments. The doc isn't written.
                let key = env::key(&envs);
                if unrouted.insert(key.clone()) {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        path: path.clone(),
                        pos: doc.source.start,
                        message: format!("No `--env` output for `{key}`."),
                    });
                }
                continue;
            };
            let i = *index.entry(name.clone()).or_insert_with(|| {
                outputs.push((name, Vec::new()));
                outputs.len() - 1
            });
            outputs[i].1.push(doc);
        }
    }

    let outputs = outputs
        .into_iter()
        .map(|(name, docs)| {
            let mut sources: Vec<String> = Vec::new();
            for doc in &docs {
                if !sources.contains(&doc.source.path) {
                    sources.push(doc.source.path.clone());
                }
            }
            let rendered = build(docs, options, &mut diagnostics);
            Output {
                name,
                sources,
                rendered,
            }
        })
        .collect();
    sort(&mut diagnostics);
    (outputs, diagnostics)
}

/// The source map: for each output, the C++ file and line each of its lines came from.
///
/// ```json
/// { "version": 2,
///   "files": { "<output>": { "sources": ["<path>", ...], "lines": [[<source>, <line>] | null, ...] } } }
/// ```
///
/// `lines` has one entry per line of the output: an index into `sources` and a 1-based C++ line,
/// or `null` for a generated line (the header, blank lines). An output made by `--env` can mix
/// lines from several sources.
pub fn source_map(outputs: &[Output]) -> Value {
    let mut files = serde_json::Map::new();
    for output in outputs {
        let mut sources: Vec<&str> = Vec::new();
        let lines: Vec<Value> = output
            .rendered
            .line_map
            .iter()
            .map(|src| match src {
                Some(src) => {
                    let i = sources
                        .iter()
                        .position(|s| *s == &*src.file)
                        .unwrap_or_else(|| {
                            sources.push(&src.file);
                            sources.len() - 1
                        });
                    json!([i, src.line])
                }
                None => Value::Null,
            })
            .collect();
        files.insert(
            output.name.clone(),
            json!({ "sources": sources, "lines": lines }),
        );
    }
    json!({ "version": 2, "files": files })
}

/// A source file's docs, without empty ones.
fn parse_file(
    path: &str,
    source: &str,
    options: &Options,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Doc> {
    scan::scan(source, &options.syntax)
        .iter()
        .map(|block| parse_doc(path, block, diagnostics))
        .filter(|doc| !doc.description.is_empty() || !doc.tags.is_empty())
        .collect()
}

/// One output file from its docs.
fn build(docs: Vec<Doc>, options: &Options, diagnostics: &mut Vec<Diagnostic>) -> Rendered {
    let decls = docs
        .into_iter()
        .flat_map(|doc| decl::interpret(doc, diagnostics))
        .collect();
    emit::emit(&library::resolve(decls), options)
}

/// Diagnostics in file and line order.
fn sort(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        (a.path.as_str(), a.pos.line, a.pos.col).cmp(&(b.path.as_str(), b.pos.line, b.pos.col))
    });
}
