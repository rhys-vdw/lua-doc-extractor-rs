//! Text as lines that remember where they came from.

use std::rc::Rc;

/// A C++ file and line (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Src {
    pub file: Rc<str>,
    pub line: usize,
}

impl Src {
    pub fn new(file: impl Into<Rc<str>>, line: usize) -> Self {
        Src {
            file: file.into(),
            line,
        }
    }
}

/// A line of text, and the C++ line it came from, if any. An output can mix lines from several
/// files, so each line keeps its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub src: Option<Src>,
}

impl Line {
    pub fn new(text: impl Into<String>, src: Option<Src>) -> Self {
        Line {
            text: text.into(),
            src,
        }
    }

    /// A line with no source, such as a blank line between paragraphs.
    pub fn generated(text: impl Into<String>) -> Self {
        Line::new(text, None)
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// Removes leading and trailing blank lines.
pub fn trimmed(mut lines: Vec<Line>) -> Vec<Line> {
    while lines.first().is_some_and(Line::is_blank) {
        lines.remove(0);
    }
    while lines.last().is_some_and(Line::is_blank) {
        lines.pop();
    }
    lines
}

/// Appends `src` to `dest` as a new paragraph.
pub fn join(dest: &mut Vec<Line>, src: Vec<Line>) {
    let src = trimmed(src);
    if src.is_empty() {
        return;
    }
    *dest = trimmed(std::mem::take(dest));
    if !dest.is_empty() {
        dest.push(Line::generated(""));
    }
    dest.extend(src);
}

/// Prefixes each line with `---`.
pub fn commented(lines: Vec<Line>, indent: &str) -> Vec<Line> {
    lines
        .into_iter()
        .map(|l| Line::new(format!("{indent}---{}", l.text), l.src))
        .collect()
}
