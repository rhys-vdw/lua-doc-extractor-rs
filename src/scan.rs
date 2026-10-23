//! Finds `/*** ... */` doc comment blocks in a source file.
//!
//! The host language is never parsed: only the comment markers are recognised.

use fancy_regex::{Match, Regex};

/// Opens a C-style doc: exactly three stars, so `/****` banners and `/***/` aren't docs.
pub const C_OPEN: &str = r"/\*{3}(?![*/])";
/// Closes a C-style comment, along with any stars before it.
pub const C_CLOSE: &str = r"\*+/";
/// The ` * ` decoration of a C-style comment's lines.
pub const C_LINE: &str = r"^\s*\*";

/// The patterns that mark a doc comment. Each match is exactly the marker, so lookaround is
/// how a pattern checks what's around it without taking it.
#[derive(Debug, Clone)]
pub struct Syntax {
    /// Opens a block. The text starts after it.
    pub open: Regex,
    /// Closes a block. The text ends before it.
    pub close: Regex,
    /// Decoration at the start of each line after the first, removed from the text.
    pub line: Regex,
}

/// C-style `/*** ... */`.
impl Default for Syntax {
    fn default() -> Self {
        Syntax {
            open: Regex::new(C_OPEN).unwrap(),
            close: Regex::new(C_CLOSE).unwrap(),
            line: Regex::new(C_LINE).unwrap(),
        }
    }
}

/// A 1-based position in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// One line of a doc comment, with its decoration (` * `) and common indent removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Where `text` starts in the source.
    pub pos: Pos,
    pub text: String,
}

/// A doc comment block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Position of the opening `/***`.
    pub start: Pos,
    /// Position of the closing `*/`.
    pub end: Pos,
    pub lines: Vec<Line>,
}

/// Extracts every block marked by `syntax` from `source`.
///
/// A block closes at the first close after its open. On every line but the first, the
/// line decoration is removed. The common indent is then removed, along with leading and
/// trailing blank lines and trailing whitespace.
pub fn scan(source: &str, syntax: &Syntax) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = source.split('\n').collect();

    let mut line_index = 0;
    let mut col = 0; // byte offset into the current line
    while line_index < lines.len() {
        let line = lines[line_index].trim_end_matches('\r');
        let Some(open) = find(&syntax.open, line, col) else {
            line_index += 1;
            col = 0;
            continue;
        };

        let start = Pos {
            line: line_index + 1,
            col: char_col(line, open.start()),
        };
        let mut body_start = Some(open.end());
        let mut raw: Vec<(Pos, String)> = Vec::new();
        let mut end = None;

        while line_index < lines.len() {
            let line = lines[line_index].trim_end_matches('\r');
            let close = find(&syntax.close, line, body_start.unwrap_or(0));
            let body_end = close.map_or(line.len(), |c| c.start());
            // The first line's text starts after the open; later lines' after their decoration.
            let text_start = body_start
                .unwrap_or_else(|| find(&syntax.line, &line[..body_end], 0).map_or(0, |m| m.end()));
            raw.push((
                Pos {
                    line: line_index + 1,
                    col: char_col(line, text_start),
                },
                line[text_start..body_end].to_string(),
            ));
            if let Some(close) = close {
                end = Some(Pos {
                    line: line_index + 1,
                    col: char_col(line, close.start()),
                });
                col = close.end();
                break;
            }
            body_start = None;
            line_index += 1;
        }

        match end {
            Some(end) => blocks.push(Block {
                start,
                end,
                lines: tidy(raw),
            }),
            // Unterminated comment: nothing more to find.
            None => break,
        }
    }

    blocks
}

/// The first match of `re` in `s` at or after byte `from`.
fn find<'s>(re: &Regex, s: &'s str, from: usize) -> Option<Match<'s>> {
    re.find_from_pos(s, from)
        .expect("doc comment pattern exceeded its backtrack limit")
}

fn char_col(line: &str, byte: usize) -> usize {
    line[..byte].chars().count() + 1
}

fn tidy(raw: Vec<(Pos, String)>) -> Vec<Line> {
    let indent = raw
        .iter()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(_, t)| t.len() - t.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut lines: Vec<Line> = raw
        .into_iter()
        .map(|(pos, text)| {
            let cut = indent.min(text.len() - text.trim_start().len());
            Line {
                pos: Pos {
                    line: pos.line,
                    col: pos.col + cut,
                },
                text: text[cut..].trim_end().to_string(),
            }
        })
        .collect();

    while lines.first().is_some_and(|l| l.text.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    // The first line keeps no leading whitespace.
    if let Some(first) = lines.first_mut() {
        let trimmed = first.text.trim_start();
        first.pos.col += first.text.len() - trimmed.len();
        first.text = trimmed.to_string();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scans `source` as C.
    fn c(source: &str) -> Vec<Block> {
        scan(source, &Syntax::default())
    }

    fn texts(block: &Block) -> Vec<&str> {
        block.lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn multi_line_block() {
        let blocks = c("int x;\n/***\n * Hello\n *   indented\n * @function Foo\n */\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(texts(&blocks[0]), ["Hello", "  indented", "@function Foo"]);
        assert_eq!(blocks[0].start, Pos { line: 2, col: 1 });
        assert_eq!(blocks[0].end, Pos { line: 6, col: 2 });
        assert_eq!(blocks[0].lines[0].pos, Pos { line: 3, col: 4 });
    }

    #[test]
    fn single_line_blocks() {
        let blocks = c("\t/*** @field CMD.OPT_ALT 128 */ /*** @field B 1 */");
        assert_eq!(blocks.len(), 2);
        assert_eq!(texts(&blocks[0]), ["@field CMD.OPT_ALT 128"]);
        assert_eq!(texts(&blocks[1]), ["@field B 1"]);
    }

    #[test]
    fn indented_block_without_decoration_on_first_line() {
        let blocks = c("\t\t/*** @function Spring.X\n\t\t * Long.\n\t\t */");
        assert_eq!(texts(&blocks[0]), ["@function Spring.X", "Long."]);
    }

    #[test]
    fn ignores_banners_and_plain_comments() {
        assert!(c("/******/\n/***/\n/* not docs */\n/** nor these */").is_empty());
    }

    #[test]
    fn ignores_banners_with_text() {
        let source = "/****************\n * Engine constants\n * @see rts/Lua/LuaConstEngine.cpp\n****************/\n";
        assert!(c(source).is_empty());
    }

    #[test]
    fn text_straight_after_the_open() {
        let blocks = c("/***Can only be called here. */");
        assert_eq!(texts(&blocks[0]), ["Can only be called here."]);
        assert_eq!(blocks[0].lines[0].pos, Pos { line: 1, col: 5 });
    }

    #[test]
    fn close_with_several_stars() {
        let blocks = c("/***\n * Hello\n **/");
        assert_eq!(texts(&blocks[0]), ["Hello"]);
        assert_eq!(blocks[0].end, Pos { line: 3, col: 2 });
    }

    #[test]
    fn other_syntax() {
        let syntax = Syntax {
            open: Regex::new(r"/\*lua\b").unwrap(),
            close: Regex::new(r"\*/").unwrap(),
            line: Regex::new(r"^\s*\*?").unwrap(),
        };
        let blocks = scan("/*lua\n  @function Foo\n */ /*** not this */", &syntax);
        assert_eq!(blocks.len(), 1);
        assert_eq!(texts(&blocks[0]), ["@function Foo"]);
    }
}
