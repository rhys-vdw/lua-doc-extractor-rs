//! Generates an emmylua library from Lua doc comments in C++ source.

pub mod doc;
pub mod emit;
pub mod name;
pub mod scan;
pub mod types;

use doc::{Diagnostic, parse_doc};
pub use emit::Options;

/// Extracts one source file into the text of one Lua file.
pub fn extract(path: &str, source: &str, options: &Options) -> (String, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let docs = scan::scan(source)
        .iter()
        .map(|block| parse_doc(path, block, &mut diagnostics))
        .filter(|doc| !doc.description.is_empty() || !doc.tags.is_empty())
        .collect();
    (emit::emit(docs, &[path.to_string()], options), diagnostics)
}
