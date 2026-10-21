//! Names of documented symbols: `Spring.GetUnitHealth`, `Foo:bar`, `CMD[1]`, `[1]`.
//!
//! This is the only grammar the extractor owns. Types come from emmylua.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// `name`
    Name(String),
    /// `[key]`, holding the key's source text, e.g. `1` or `"weird-key"`.
    Index(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Segment>,
    /// The last segment follows a `:`.
    pub is_method: bool,
}

impl Path {
    /// The path without its last segment, e.g. the table of a field.
    pub fn parent(&self) -> Option<Path> {
        (self.segments.len() > 1).then(|| Path {
            segments: self.segments[..self.segments.len() - 1].to_vec(),
            is_method: false,
        })
    }

    pub fn last(&self) -> Path {
        Path {
            segments: vec![self.segments.last().unwrap().clone()],
            is_method: false,
        }
    }
}

/// Parses a path at the start of `s`, returning it and the rest of the text.
pub fn parse_path(s: &str) -> Option<(Path, &str)> {
    let mut rest = s;
    let mut segments = Vec::new();
    let mut is_method = false;

    loop {
        let first = segments.is_empty();
        if let Some(after) = rest.strip_prefix('[') {
            let close = find_index_close(after)?;
            segments.push(Segment::Index(after[..close].trim().to_string()));
            rest = &after[close + 1..];
        } else if first {
            let (name, after) = identifier(rest)?;
            segments.push(Segment::Name(name.to_string()));
            rest = after;
        } else if let Some(after) = rest.strip_prefix('.') {
            let (name, after) = identifier(after)?;
            segments.push(Segment::Name(name.to_string()));
            rest = after;
        } else if let Some(after) = rest.strip_prefix(':') {
            is_method = true;
            let (name, after) = identifier(after)?;
            segments.push(Segment::Name(name.to_string()));
            rest = after;
            break;
        } else {
            break;
        }
    }

    // A path must end at a word boundary.
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some((
        Path {
            segments,
            is_method,
        },
        rest,
    ))
}

fn identifier(s: &str) -> Option<(&str, &str)> {
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())))
        .map_or(s.len(), |(i, _)| i);
    (end > 0).then(|| s.split_at(end))
}

/// Finds the `]` closing an index, skipping over quoted strings.
fn find_index_close(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, ']') => return Some(i),
            _ => {}
        }
    }
    None
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// Formats the path as Lua, quoting keywords: `Foo["end"]`.
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let last = self.segments.len() - 1;
        for (i, segment) in self.segments.iter().enumerate() {
            match segment {
                Segment::Index(key) => write!(f, "[{key}]")?,
                Segment::Name(name) if is_keyword(name) => write!(f, "[\"{name}\"]")?,
                Segment::Name(name) if i == 0 => write!(f, "{name}")?,
                Segment::Name(name) if i == last && self.is_method => write!(f, ":{name}")?,
                Segment::Name(name) => write!(f, ".{name}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(s: &str) -> (String, &str) {
        let (path, rest) = parse_path(s).unwrap();
        (path.to_string(), rest)
    }

    #[test]
    fn dotted() {
        assert_eq!(
            roundtrip("Spring.MoveCtrl.Enabled boolean"),
            ("Spring.MoveCtrl.Enabled".into(), " boolean")
        );
    }

    #[test]
    fn method() {
        let (path, rest) = parse_path("RmlUi.Element:AppendChild rest").unwrap();
        assert!(path.is_method);
        assert_eq!(path.to_string(), "RmlUi.Element:AppendChild");
        assert_eq!(rest, " rest");
    }

    #[test]
    fn indexed() {
        assert_eq!(roundtrip("Foo[1] integer"), ("Foo[1]".into(), " integer"));
        assert_eq!(
            roundtrip(r#"Foo["a]b"].c x"#),
            (r#"Foo["a]b"].c"#.into(), " x")
        );
        assert_eq!(roundtrip("[1] integer"), ("[1]".into(), " integer"));
    }

    #[test]
    fn keywords_are_quoted() {
        assert_eq!(roundtrip("Foo.end"), (r#"Foo["end"]"#.into(), ""));
    }

    #[test]
    fn rejects_non_names() {
        assert!(parse_path("1abc").is_none());
        assert!(parse_path("").is_none());
    }
}
