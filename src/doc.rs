//! Turns a comment block into a doc: a description and a list of tags.
//!
//! Only tags that generate code are understood. Every other tag is kept as text.

use crate::name::{Path, parse_path};
use crate::scan::{Block, Pos};
use crate::types::{Typed, parse_type};
use emmylua_parser::{LuaParser, LuaSyntaxKind, ParserConfig};

/// Where a doc came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub path: String,
    pub start: Pos,
    pub end: Pos,
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub source: Source,
    pub description: Vec<String>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone)]
pub enum Tag {
    /// `@function Spring.X [description]`
    Function { path: Path, description: Vec<String> },
    /// `@table Name [description]`
    Table { path: Path, description: Vec<String> },
    /// `@field Table.name type [description]` or `@global name type [description]`.
    Field { path: Path, typed: Typed, raw: Raw, source: Source },
    /// `@enum Name ...`
    Enum { path: Path, raw: Raw },
    /// `@class Name ...`
    Class { path: Path, raw: Raw },
    /// `@param name ...`
    Param { name: String, raw: Raw },
    /// Any other tag, kept as written.
    Other(Raw),
    /// A description that isn't part of a tag's line. emmylua attaches it to the next tag,
    /// if any, so it's kept in place.
    Text(Vec<String>),
}

/// A tag as written: its name, the rest of its line, and its continuation lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub path: String,
    pub pos: Pos,
    pub message: String,
}

pub fn parse_doc(path: &str, block: &Block, diagnostics: &mut Vec<Diagnostic>) -> Doc {
    let source = Source { path: path.to_string(), start: block.start, end: block.end };
    let parsed = split(block);
    let mut report = |severity, line: usize, message: String| {
        diagnostics.push(Diagnostic { severity, path: path.to_string(), pos: block.lines[line].pos, message })
    };
    for (line, message) in &parsed.errors {
        report(Severity::Error, *line, message.clone());
    }

    let mut description = Vec::new();
    let mut tags = Vec::new();
    for (i, item) in parsed.items.iter().enumerate() {
        let mut lines: Vec<String> = block.lines[item.start..=item.end].iter().map(|l| l.text.clone()).collect();
        while lines.len() > 1 && lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        match item.kind {
            ItemKind::Description => description = lines,
            ItemKind::Text => tags.push(Tag::Text(lines)),
            ItemKind::Tag => {
                let (name, rest) = tag_start(&lines[0]).expect("a tag item starts with a tag");
                let mut raw_lines = vec![rest.trim_start().to_string()];
                raw_lines.extend(lines[1..].iter().cloned());
                let raw = Raw { name: name.to_string(), lines: raw_lines };

                for message in tag_warnings(&raw, &parsed.items, i, block) {
                    report(Severity::Warning, item.start, message);
                }
                let has_error = parsed.errors.iter().any(|(l, _)| (item.start..=item.end).contains(l));
                tags.push(to_tag(raw, &source).unwrap_or_else(|(raw, message)| {
                    // emmylua's error already covers this line.
                    if !has_error {
                        report(Severity::Error, item.start, message);
                    }
                    Tag::Other(raw)
                }));
            }
        }
    }

    Doc { source, description, tags }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    /// The comment's own description, before any tag.
    Description,
    /// A tag, with its inline description.
    Tag,
    /// A description after a tag that isn't on that tag's line.
    Text,
}

/// A run of lines in a block (indices into `block.lines`, inclusive).
#[derive(Debug, Clone, Copy)]
struct Item {
    kind: ItemKind,
    start: usize,
    end: usize,
    /// For a tag: it has text after its name or type on its own line.
    inline: bool,
}

struct Split {
    items: Vec<Item>,
    /// emmylua parse errors, by line.
    errors: Vec<(usize, String)>,
}

/// Splits a block into its description, tags and free text, as emmylua reads it.
///
/// A tag owns its inline description: text on the tag's line, and the plain lines that
/// follow it. A description that starts on a later line is free text, which emmylua
/// attaches to the next tag.
fn split(block: &Block) -> Split {
    let mut text = String::new();
    let mut line_starts = Vec::new();
    for line in &block.lines {
        line_starts.push(text.len());
        text.push_str("---");
        text.push_str(&line.text);
        text.push('\n');
    }
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset).saturating_sub(1);

    let tree = LuaParser::parse(&text, ParserConfig::default());
    let errors = tree
        .get_errors()
        .iter()
        .map(|e| (line_of(usize::from(e.range.start())), e.message.to_string()))
        .collect();

    let mut items: Vec<Item> = Vec::new();
    let comment = tree.get_red_root().descendants().find(|n| n.kind() == LuaSyntaxKind::Comment.into());
    for node in comment.iter().flat_map(|c| c.children()) {
        let range = node.text_range();
        let start = line_of(usize::from(range.start()));
        let end = line_of(usize::from(range.end()).saturating_sub(1)).max(start);
        let kind: LuaSyntaxKind = node.kind().into();

        let last = items.last_mut();
        if format!("{kind:?}").starts_with("DocTag") {
            items.push(Item { kind: ItemKind::Tag, start, end, inline: false });
        } else if kind == LuaSyntaxKind::DocDescription {
            // emmylua leaves empty descriptions after some tags.
            if node.text().to_string().replace("---", "").trim().is_empty() {
                continue;
            }
            match last {
                Some(tag) if tag.kind == ItemKind::Tag && start == tag.end => {
                    tag.inline = true;
                    tag.end = tag.end.max(end);
                }
                None => items.push(Item { kind: ItemKind::Description, start, end, inline: false }),
                Some(_) => items.push(Item { kind: ItemKind::Text, start, end, inline: false }),
            }
        } else if let Some(last) = last {
            last.end = last.end.max(end);
        }
    }

    // Each item runs up to the next one, so lines emmylua left out (after a parse error,
    // say) stay with the item above them.
    items.dedup_by(|b, a| {
        let same = b.start <= a.start;
        if same && b.kind == ItemKind::Tag {
            *a = *b;
        }
        same
    });
    let count = items.len();
    for i in 0..count {
        items[i].end = if i + 1 < count { items[i + 1].start - 1 } else { block.lines.len() - 1 };
    }
    if let Some(first) = items.first()
        && first.start > 0
    {
        items.insert(0, Item { kind: ItemKind::Description, start: 0, end: first.start - 1, inline: false });
    }
    Split { items, errors }
}

/// Markup that emmylua reads differently from how it looks.
fn tag_warnings(raw: &Raw, items: &[Item], i: usize, block: &Block) -> Vec<String> {
    let mut warnings = Vec::new();
    let item = &items[i];
    let next = items.get(i + 1);
    let text_follows = next.is_some_and(|n| n.kind == ItemKind::Text);
    // No blank line between the tag and the text.
    let directly_below =
        |next: Option<&Item>| next.is_some_and(|n| !block.lines[n.start - 1].text.trim().is_empty());
    // Text is attached to nothing when no tag follows it, or when the next tag only
    // reads its inline description.
    let after_text = items.get(i + 2).and_then(|n| tag_start(&block.lines[n.start].text)).map(|(name, _)| name);
    let text_is_unattached =
        text_follows && !after_text.is_some_and(|name| matches!(name, "field" | "class" | "enum" | "alias"));

    match raw.name.as_str() {
        "param" | "return" => {
            let continued = raw.lines[1..].iter().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('|'));
            if continued || text_follows {
                warnings.push(format!(
                    "@{} description must be on one line, on the tag line; move longer text to the function description",
                    raw.name
                ));
            }
        }
        // Text after a blank line describes the next tag, which is emmylua's convention.
        "field" | "class" | "enum" | "alias"
            if !item.inline && text_follows && (directly_below(next) || text_is_unattached) =>
        {
            warnings.push(format!(
                "text below @{} belongs to the next tag in emmylua; put it on the tag line, or above \
                 the tag",
                raw.name
            ));
        }
        _ => {}
    }
    warnings
}

/// A line starting with `@name` begins a tag.
fn tag_start(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim_start().strip_prefix('@')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    (end > 0).then(|| rest.split_at(end))
}

fn to_tag(raw: Raw, source: &Source) -> Result<Tag, (Raw, String)> {
    let first = raw.lines[0].as_str();
    let fail = |raw: Raw, message: String| Err((raw, message));

    match raw.name.as_str() {
        "function" | "table" => {
            let Some((path, rest)) = parse_path(first) else {
                return fail(raw.clone(), format!("expected a name after @{}", raw.name));
            };
            let description = continuation(rest, &raw.lines[1..]);
            Ok(if raw.name == "function" {
                Tag::Function { path, description }
            } else {
                Tag::Table { path, description }
            })
        }
        "field" | "global" => {
            let Some((path, rest)) = parse_path(first) else {
                return fail(raw.clone(), format!("expected a name after @{}", raw.name));
            };
            let mut lines = vec![rest];
            lines.extend(raw.lines[1..].iter().map(String::as_str));
            match parse_type(&lines) {
                Ok(typed) => Ok(Tag::Field { path, typed, raw, source: source.clone() }),
                Err(message) => fail(raw.clone(), format!("invalid type in @{}: {message}", raw.name)),
            }
        }
        "enum" => match parse_path(first) {
            Some((path, _)) => Ok(Tag::Enum { path, raw }),
            None => fail(raw.clone(), "expected a name after @enum".into()),
        },
        "class" => {
            // Skip a flag such as `(exact)`.
            let name = match first.strip_prefix('(') {
                Some(after) => after.split_once(')').map_or("", |(_, r)| r.trim_start()),
                None => first,
            };
            match parse_path(name) {
                Some((path, _)) => Ok(Tag::Class { path, raw }),
                None => fail(raw.clone(), "expected a name after @class".into()),
            }
        }
        "param" => {
            // `name`, `name?` or `...`, followed by whitespace. Anything else is an
            // error: the tag is kept as written, but it's left out of the signature.
            let word = first.split_whitespace().next().unwrap_or("");
            let name = word.strip_suffix('?').unwrap_or(word);
            let valid = name == "..."
                || parse_path(name).is_some_and(|(path, rest)| rest.is_empty() && path.segments.len() == 1);
            if !valid {
                return fail(raw.clone(), format!("invalid @param name '{word}'"));
            }
            Ok(Tag::Param { name: name.to_string(), raw })
        }
        _ => Ok(Tag::Other(raw)),
    }
}

/// The rest of a tag's first line, followed by its continuation lines.
fn continuation(rest: &str, more: &[String]) -> Vec<String> {
    let mut lines = vec![rest.trim().to_string()];
    lines.extend(more.iter().cloned());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn parse(source: &str) -> (Doc, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let block = &scan(source)[0];
        (parse_doc("test.cpp", block, &mut diagnostics), diagnostics)
    }

    #[test]
    fn see_then_enum() {
        let (doc, diagnostics) = parse("/***\n * Command constants.\n *\n * @see Spring.A\n * @see Spring.B\n * @enum CMD\n */");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(doc.description, ["Command constants."]);
        assert!(matches!(doc.tags.last(), Some(Tag::Enum { .. })), "{:#?}", doc.tags);
    }
}
