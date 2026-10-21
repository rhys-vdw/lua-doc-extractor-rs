//! Finds description lines that emmylua and the spike's line split assign differently.
//!
//! cargo run --example survey -- <files...>
//!
//! The spike gives plain lines after a tag to that tag. emmylua does the same only if
//! the tag has text on its own line; otherwise the lines are a separate description
//! that belongs to the *next* tag.

use std::collections::BTreeMap;

use emmylua_parser::{LuaKind, LuaParser, LuaSyntaxKind, LuaTokenKind, ParserConfig};
use lua_doc_extractor::scan::scan;

fn main() {
    let mut counts: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for path in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&path).unwrap();
        for block in scan(&source) {
            let text: String = block
                .lines
                .iter()
                .map(|l| format!("---{}\n", l.text))
                .collect();
            let tree = LuaParser::parse(&text, ParserConfig::default());
            let Some(comment) = tree
                .get_red_root()
                .descendants()
                .find(|n| n.kind() == LuaSyntaxKind::Comment.into())
            else {
                continue;
            };

            let children: Vec<_> = comment.children_with_tokens().collect();
            let mut prev_tag: Option<String> = None;
            for (i, child) in children.iter().enumerate() {
                let LuaKind::Syntax(kind) = child.kind() else {
                    continue;
                };
                if kind == LuaSyntaxKind::DocDescription {
                    let Some(prev) = &prev_tag else { continue };
                    let snippet = child
                        .to_string()
                        .replace("---", "")
                        .trim()
                        .chars()
                        .take(60)
                        .collect::<String>();
                    if snippet.is_empty() {
                        continue;
                    }
                    if is_inline(&children, i) {
                        continue;
                    }
                    let next = children[i + 1..]
                        .iter()
                        .find_map(tag_name)
                        .unwrap_or_else(|| "(end)".to_string());
                    let line = text[..usize::from(child.text_range().start())]
                        .matches('\n')
                        .count();
                    let pos = block.lines[line.min(block.lines.len() - 1)].pos;
                    counts
                        .entry((prev.clone(), next))
                        .or_default()
                        .push(format!("{path}:{}  {snippet}", pos.line));
                } else if let Some(name) = tag_name(child) {
                    prev_tag = Some(name);
                }
            }
        }
    }

    for ((prev, next), places) in &counts {
        println!("{:4}  after @{prev}, before {next}", places.len());
        for p in places.iter().take(3) {
            println!("        {p}");
        }
    }
}

/// A description is inline when something other than whitespace precedes it on its line.
fn is_inline(children: &[emmylua_parser::LuaSyntaxElement], i: usize) -> bool {
    for c in children[..i].iter().rev() {
        match c.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkDocContinue) => {}
            LuaKind::Token(LuaTokenKind::TkEndOfLine | LuaTokenKind::TkNormalStart) => {
                return false;
            }
            _ => return true,
        }
    }
    false
}

/// `@name` for a tag node, from its first token.
fn tag_name(element: &emmylua_parser::LuaSyntaxElement) -> Option<String> {
    let node = element.as_node()?;
    let name = format!("{:?}", node.kind());
    if !name.contains("DocTag") {
        return None;
    }
    let token = node.first_token()?;
    Some(token.text().to_string())
}
