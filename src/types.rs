//! Splits typed text (`integer The unit.`) into a type and a description, using
//! emmylua's parser.
//!
//! `emmylua_parser` only makes whole-chunk parsing public, so the text is parsed as
//! the body of a `---@type` tag. This is the only place that does so.

use emmylua_parser::{
    LuaAstNode, LuaDocTagType, LuaParser, LuaSyntaxKind, ParserConfig,
};

const PREFIX: &str = "---@type ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    /// The type, one entry per source line.
    pub ty: Vec<String>,
    /// The literal value, if the type is one (`128`, `"name"`, `true`).
    pub literal: Option<String>,
    /// Everything after the type.
    pub description: Vec<String>,
}

/// Parses `lines` (the text after a name, then any continuation lines) as a type and
/// a description.
pub fn parse_type(lines: &[&str]) -> Result<Typed, String> {
    let mut text = String::from(PREFIX);
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            text.push_str("\n---");
        }
        text.push_str(line);
    }

    let tree = LuaParser::parse(&text, ParserConfig::default());
    if let Some(error) = tree.get_errors().first() {
        return Err(error.message.to_string());
    }

    let tag = tree
        .get_red_root()
        .descendants()
        .find_map(LuaDocTagType::cast)
        .ok_or("no type")?;
    let types: Vec<_> = tag.get_type_list().collect();
    let [ty] = types.as_slice() else {
        return Err(format!("expected one type, found {}", types.len()));
    };

    let range = ty.syntax().text_range();
    let start = locate(lines, usize::from(range.start()));
    let end = locate(lines, usize::from(range.end()));

    let mut ty_lines = slice(lines, start, end);
    if start.0 > 0 {
        // The type starts on a later line (a multi-line union).
        ty_lines.insert(0, String::new());
    }
    let literal = is_literal(ty.syntax()).then(|| ty_lines.join("\n"));
    let after = slice(lines, end, (lines.len() - 1, lines.last().map_or(0, |l| l.len())));

    Ok(Typed { ty: ty_lines, literal, description: trim_lines(after) })
}

/// Parses a tag that is passed through unchanged, returning emmylua's first parse error.
/// `lines` are the text after `@name`, then any continuation lines.
pub fn check_tag(name: &str, lines: &[String]) -> Result<(), String> {
    let mut text = format!("---@{name} ");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            text.push_str("\n---");
        }
        text.push_str(line);
    }
    let tree = LuaParser::parse(&text, ParserConfig::default());
    match tree.get_errors().first() {
        Some(error) => Err(error.message.to_string()),
        None => Ok(()),
    }
}

/// A number, string or boolean literal, including negative numbers.
fn is_literal(node: &emmylua_parser::LuaSyntaxNode) -> bool {
    match node.kind().into() {
        LuaSyntaxKind::TypeLiteral => true,
        LuaSyntaxKind::TypeUnary => {
            node.text().to_string().starts_with('-')
                && node.children().all(|c| c.kind() == LuaSyntaxKind::TypeLiteral.into())
        }
        _ => false,
    }
}

/// Maps an offset in the parsed text to (line, byte column) in `lines`. Offsets
/// inside an added prefix map to the start of the line.
fn locate(lines: &[&str], offset: usize) -> (usize, usize) {
    let mut line_start = 0;
    for (i, line) in lines.iter().enumerate() {
        let prefix = if i == 0 { PREFIX.len() } else { 3 };
        let line_end = line_start + prefix + line.len();
        if offset <= line_end {
            return (i, offset.saturating_sub(line_start + prefix).min(line.len()));
        }
        line_start = line_end + 1;
    }
    (lines.len() - 1, lines.last().map_or(0, |l| l.len()))
}

fn slice(lines: &[&str], start: (usize, usize), end: (usize, usize)) -> Vec<String> {
    (start.0..=end.0)
        .map(|i| {
            let from = if i == start.0 { start.1 } else { 0 };
            let to = if i == end.0 { end.1 } else { lines[i].len() };
            lines[i][from..to.max(from)].to_string()
        })
        .collect()
}

fn trim_lines(mut lines: Vec<String>) -> Vec<String> {
    if let Some(first) = lines.first_mut() {
        *first = first.trim_start().to_string();
    }
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Typed {
        parse_type(&s.split('\n').collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn named_type_with_description() {
        let t = parse("integer The unit.");
        assert_eq!(t.ty, ["integer"]);
        assert_eq!(t.literal, None);
        assert_eq!(t.description, ["The unit."]);
    }

    #[test]
    fn literals() {
        assert_eq!(parse("128").literal.as_deref(), Some("128"));
        assert_eq!(parse("-1").literal.as_deref(), Some("-1"));
        assert_eq!(parse(r#""name" desc"#).literal.as_deref(), Some(r#""name""#));
        assert_eq!(parse("true").literal.as_deref(), Some("true"));
    }

    #[test]
    fn table_literal_type_is_not_a_literal_value() {
        let t = parse("{ x: integer, y: integer } A point.");
        assert_eq!(t.ty, ["{ x: integer, y: integer }"]);
        assert_eq!(t.literal, None);
        assert_eq!(t.description, ["A point."]);
    }

    #[test]
    fn multi_line_description() {
        let t = parse("0\n\nStop the current action.\n\nAccepts no parameters.");
        assert_eq!(t.literal.as_deref(), Some("0"));
        assert_eq!(t.description, ["Stop the current action.", "", "Accepts no parameters."]);
    }

    #[test]
    fn multi_line_union() {
        let t = parse("\n| \"a\" # First\n| \"b\" # Second");
        assert_eq!(t.ty, ["", "| \"a\" # First", "| \"b\" # Second"]);
    }

    #[test]
    fn several_types_is_an_error() {
        assert!(parse_type(&["integer, string"]).is_err());
    }
}
