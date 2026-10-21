//! Generates an emmylua library from Lua doc comments in C++ source.
//!
//! ```text
//! C++ ─scan→ Block ─parse→ Doc ─interpret→ Decl ─resolve→ Item ─emit→ Lua
//! ```
//!
//! Each stage is a function from one value to the next. A `Doc` is what was written, a
//! `Decl` is what one doc declares, and an `Item` is a declaration placed in the library.

pub mod decl;
pub mod doc;
pub mod emit;
pub mod library;
pub mod name;
pub mod scan;
mod text;
pub mod types;

use doc::{Diagnostic, parse_doc};
pub use emit::{Options, Rendered};

/// Extracts one source file into one Lua file, with its line map.
pub fn extract(path: &str, source: &str, options: &Options) -> (Rendered, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let docs: Vec<_> = scan::scan(source)
        .iter()
        .map(|block| parse_doc(path, block, &mut diagnostics))
        .filter(|doc| !doc.description.is_empty() || !doc.tags.is_empty())
        .collect();
    let decls = docs
        .into_iter()
        .flat_map(|doc| decl::interpret(doc, &mut diagnostics))
        .collect();
    let items = library::resolve(decls);
    diagnostics.sort_by_key(|d| (d.pos.line, d.pos.col));
    (emit::emit(&items, options), diagnostics)
}
