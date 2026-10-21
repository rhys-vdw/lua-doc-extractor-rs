//! Finds `/*** ... */` doc comment blocks in a source file.
//!
//! The host language is never parsed: only the comment markers are recognised.

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

/// Extracts every `/***` block from `source`.
///
/// A block opens with `/` and three or more `*` (but not `/***/`), and closes at the
/// next `*/`. On every line but the first, leading whitespace and a single `*` are
/// removed. The common indent is then removed, along with leading and trailing blank
/// lines and trailing whitespace.
pub fn scan(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = source.split('\n').collect();

    let mut line_index = 0;
    let mut col = 0; // byte offset into the current line
    while line_index < lines.len() {
        let line = lines[line_index];
        let Some(open) = find_open(&line[col..]).map(|o| o + col) else {
            line_index += 1;
            col = 0;
            continue;
        };

        let start = Pos { line: line_index + 1, col: char_col(line, open) };
        let mut body_start = open + 1 + count_stars(&line[open + 1..]);
        let mut raw: Vec<(Pos, String)> = Vec::new();
        let mut first = true;
        let mut end = None;

        while line_index < lines.len() {
            let line = lines[line_index].trim_end_matches('\r');
            let mut text_start = body_start.min(line.len());
            if !first {
                text_start = strip_decoration(line);
            }
            let rest = &line[text_start..];
            if let Some(close) = find_close(rest) {
                let close_abs = text_start + close.start;
                raw.push((
                    Pos { line: line_index + 1, col: char_col(line, text_start) },
                    rest[..close.start].to_string(),
                ));
                end = Some(Pos { line: line_index + 1, col: char_col(line, close_abs) });
                col = text_start + close.end;
                break;
            }
            raw.push((
                Pos { line: line_index + 1, col: char_col(line, text_start) },
                rest.to_string(),
            ));
            first = false;
            body_start = 0;
            line_index += 1;
        }

        match end {
            Some(end) => blocks.push(Block { start, end, lines: tidy(raw) }),
            // Unterminated comment: nothing more to find.
            None => break,
        }
    }

    blocks
}

/// Finds `/***` that isn't a `/****/` banner.
fn find_open(s: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(i) = s[search..].find("/***").map(|i| i + search) {
        let stars = count_stars(&s[i + 1..]);
        if !s[i + 1 + stars..].starts_with('/') {
            return Some(i);
        }
        search = i + 1 + stars;
    }
    None
}

struct Close {
    start: usize,
    end: usize,
}

/// Finds `*+/`, returning the range of the whole run.
fn find_close(s: &str) -> Option<Close> {
    let slash = s.find("*/")? + 1;
    let start = s[..slash].trim_end_matches('*').len();
    Some(Close { start, end: slash + 1 })
}

fn count_stars(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b'*').count()
}

/// Byte offset of the text after leading whitespace and one `*` (unless it closes the
/// comment).
fn strip_decoration(line: &str) -> usize {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    if rest.starts_with('*') && !rest.starts_with("*/") {
        indent + 1
    } else {
        0
    }
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
                pos: Pos { line: pos.line, col: pos.col + cut },
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

    fn texts(block: &Block) -> Vec<&str> {
        block.lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn multi_line_block() {
        let blocks = scan("int x;\n/***\n * Hello\n *   indented\n * @function Foo\n */\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(texts(&blocks[0]), ["Hello", "  indented", "@function Foo"]);
        assert_eq!(blocks[0].start, Pos { line: 2, col: 1 });
        assert_eq!(blocks[0].end, Pos { line: 6, col: 2 });
        assert_eq!(blocks[0].lines[0].pos, Pos { line: 3, col: 4 });
    }

    #[test]
    fn single_line_blocks() {
        let blocks = scan("\t/*** @field CMD.OPT_ALT 128 */ /*** @field B 1 */");
        assert_eq!(blocks.len(), 2);
        assert_eq!(texts(&blocks[0]), ["@field CMD.OPT_ALT 128"]);
        assert_eq!(texts(&blocks[1]), ["@field B 1"]);
    }

    #[test]
    fn indented_block_without_decoration_on_first_line() {
        let blocks = scan("\t\t/*** @function Spring.X\n\t\t * Long.\n\t\t */");
        assert_eq!(texts(&blocks[0]), ["@function Spring.X", "Long."]);
    }

    #[test]
    fn ignores_banners_and_plain_comments() {
        assert!(scan("/******/\n/* not docs */\n/** nor these */").is_empty());
    }
}
