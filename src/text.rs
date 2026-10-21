//! Helpers for description text, held as lines.

/// Removes leading and trailing blank lines.
pub fn trimmed(mut lines: Vec<String>) -> Vec<String> {
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// Appends `src` to `dest` as a new paragraph.
pub fn join(dest: &mut Vec<String>, src: Vec<String>) {
    let src = trimmed(src);
    if src.is_empty() {
        return;
    }
    *dest = trimmed(std::mem::take(dest));
    if !dest.is_empty() {
        dest.push(String::new());
    }
    dest.extend(src);
}

/// Prefixes each line with `---`.
pub fn lua_comment(lines: &[String], indent: &str) -> String {
    lines.iter().map(|l| format!("{indent}---{l}")).collect::<Vec<_>>().join("\n")
}
