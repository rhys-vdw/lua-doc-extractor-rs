//! Turns a comment block into a doc: a description and a list of tags.
//!
//! Only tags that generate code are understood. Every other tag is kept as text.

use crate::name::{Path, parse_path};
use crate::scan::{Block, Pos};
use crate::types::{Typed, check_tag, parse_type};

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
    Table { path: Path, is_local: bool, description: Vec<String> },
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
}

/// A tag as written: its name, the rest of its line, and its continuation lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub pos: Pos,
    pub message: String,
}

pub fn parse_doc(path: &str, block: &Block, diagnostics: &mut Vec<Diagnostic>) -> Doc {
    let source = Source { path: path.to_string(), start: block.start, end: block.end };
    let mut description = Vec::new();
    let mut raws: Vec<(Pos, Raw)> = Vec::new();

    for line in &block.lines {
        if let Some((name, rest)) = tag_start(&line.text) {
            raws.push((
                line.pos,
                Raw { name: name.to_string(), lines: vec![rest.trim_start().to_string()] },
            ));
        } else if let Some((_, raw)) = raws.last_mut() {
            raw.lines.push(line.text.clone());
        } else {
            description.push(line.text.clone());
        }
    }

    let tags = raws
        .into_iter()
        .map(|(pos, raw)| {
            let mut report = |message| diagnostics.push(Diagnostic { path: path.to_string(), pos, message });
            let checked = check_tag(&raw.name, &raw.lines);
            if let Err(message) = &checked {
                report(format!("in @{}: {message}", raw.name));
            }
            to_tag(raw, &source).unwrap_or_else(|(raw, message)| {
                // emmylua's error already covers this line.
                if checked.is_ok() {
                    report(message);
                }
                Tag::Other(raw)
            })
        })
        .collect();

    Doc { source, description, tags }
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
                Tag::Table { path, is_local: false, description }
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
