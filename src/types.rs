//! Splits typed text (`integer The unit.`) into a type and a description, using
//! emmylua's parser.
//!
//! `emmylua_parser` only makes whole-chunk parsing public, so the text is parsed as the body of a
//! `---@type` tag. This is the only place that does so (workaround E, UPSTREAM.md #4).

use emmylua_parser::{LuaAstNode, LuaDocTagType, LuaParser, LuaSyntaxKind, ParserConfig};

use crate::text::Line;

const PREFIX: &str = "---@type ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    /// The type, one entry per source line.
    pub ty: Vec<Line>,
    /// The literal value, if the type is one (`128`, `"name"`, `true`).
    pub literal: Option<String>,
    /// Everything after the type.
    pub description: Vec<Line>,
}

/// Parses `input` (the text after a name, then any continuation lines) as a type and
/// a description. Each part keeps the source line of the text it came from.
pub fn parse_type(input: &[Line]) -> Result<Typed, String> {
    let lines: Vec<&str> = input.iter().map(|l| l.text.as_str()).collect();
    let lines = lines.as_slice();
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
    let mut start = locate(lines, usize::from(range.start()));
    let end = locate(lines, usize::from(range.end()));
    if start.0 == 0 {
        // A type on the tag's line starts at its first character. The node can start later:
        // the `(` of `(A|B)?` isn't part of it (workaround H, UPSTREAM.md #14).
        start.1 = lines[0].len() - lines[0].trim_start().len();
    }

    let mut ty_lines = slice(input, start, end);
    if start.0 > 0 {
        // The type starts on a later line (a multi-line union).
        ty_lines.insert(0, Line::new("", input[0].src));
    }
    let ty_text: Vec<&str> = ty_lines.iter().map(|l| l.text.as_str()).collect();
    let literal = is_literal(ty.syntax()).then(|| ty_text.join("\n"));
    // emmylua ends a type at `?`, so `T?|U` and `T?[]` silently become type `T?` and a
    // description starting `|U` or `[]` (UPSTREAM.md #13).
    let rest = lines[end.0][end.1..].trim_start();
    if ty_text.last().is_some_and(|l| l.ends_with('?')) && rest.starts_with(['|', '[']) {
        return Err(format!(
            "emmylua ends the type at `?`, so `{}{rest}` isn't read as one type; \
             write `(T|U)?` or `(T?)[]`",
            ty_text.join("")
        ));
    }
    let after = slice(
        input,
        end,
        (lines.len() - 1, lines.last().map_or(0, |l| l.len())),
    );

    Ok(Typed {
        ty: ty_lines,
        literal,
        description: trim_lines(after),
    })
}

/// A number, string or boolean literal, including negative numbers.
fn is_literal(node: &emmylua_parser::LuaSyntaxNode) -> bool {
    match node.kind().into() {
        LuaSyntaxKind::TypeLiteral => true,
        LuaSyntaxKind::TypeUnary => {
            node.text().to_string().starts_with('-')
                && node
                    .children()
                    .all(|c| c.kind() == LuaSyntaxKind::TypeLiteral.into())
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
            return (
                i,
                offset.saturating_sub(line_start + prefix).min(line.len()),
            );
        }
        line_start = line_end + 1;
    }
    (lines.len() - 1, lines.last().map_or(0, |l| l.len()))
}

fn slice(lines: &[Line], start: (usize, usize), end: (usize, usize)) -> Vec<Line> {
    (start.0..=end.0)
        .map(|i| {
            let text = &lines[i].text;
            let from = if i == start.0 { start.1 } else { 0 };
            let to = if i == end.0 { end.1 } else { text.len() };
            Line::new(&text[from..to.max(from)], lines[i].src)
        })
        .collect()
}

fn trim_lines(mut lines: Vec<Line>) -> Vec<Line> {
    if let Some(first) = lines.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    crate::text::trimmed(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `s`, giving line `i` the source line `i + 1`.
    fn parse(s: &str) -> Typed {
        parse_type(&input(s)).unwrap()
    }

    fn input(s: &str) -> Vec<Line> {
        s.split('\n')
            .enumerate()
            .map(|(i, l)| Line::new(l, Some(i + 1)))
            .collect()
    }

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn named_type_with_description() {
        let t = parse("integer The unit.");
        assert_eq!(texts(&t.ty), ["integer"]);
        assert_eq!(t.literal, None);
        assert_eq!(texts(&t.description), ["The unit."]);
    }

    #[test]
    fn literals() {
        assert_eq!(parse("128").literal.as_deref(), Some("128"));
        assert_eq!(parse("-1").literal.as_deref(), Some("-1"));
        assert_eq!(
            parse(r#""name" desc"#).literal.as_deref(),
            Some(r#""name""#)
        );
        assert_eq!(parse("true").literal.as_deref(), Some("true"));
    }

    #[test]
    fn table_literal_type_is_not_a_literal_value() {
        let t = parse("{ x: integer, y: integer } A point.");
        assert_eq!(texts(&t.ty), ["{ x: integer, y: integer }"]);
        assert_eq!(t.literal, None);
        assert_eq!(texts(&t.description), ["A point."]);
    }

    #[test]
    fn parenthesised_type() {
        let t = parse("(integer|table<string, boolean>|Foo)? The value.");
        assert_eq!(texts(&t.ty), ["(integer|table<string, boolean>|Foo)?"]);
        assert_eq!(texts(&t.description), ["The value."]);
    }

    #[test]
    fn multi_line_description() {
        let t = parse("0\n\nStop the current action.\n\nAccepts no parameters.");
        assert_eq!(t.literal.as_deref(), Some("0"));
        assert_eq!(
            texts(&t.description),
            ["Stop the current action.", "", "Accepts no parameters."]
        );
        // Each part keeps the source line of its text.
        assert_eq!(
            t.description.iter().map(|l| l.src).collect::<Vec<_>>(),
            [Some(3), Some(4), Some(5)]
        );
    }

    #[test]
    fn multi_line_union() {
        let t = parse("\n| \"a\" # First\n| \"b\" # Second");
        assert_eq!(texts(&t.ty), ["", "| \"a\" # First", "| \"b\" # Second"]);
    }

    #[test]
    fn several_types_is_an_error() {
        assert!(parse_type(&input("integer, string")).is_err());
    }
}
