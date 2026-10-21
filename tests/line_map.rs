//! Each generated line maps to the C++ line it came from.

use lua_doc_extractor::{Options, extract};

/// The generated body, each line prefixed with its C++ line number (or `-`).
fn mapped(cpp: &str) -> String {
    let options = Options {
        repo: Some("https://example.com/".into()),
    };
    let (rendered, diagnostics) = extract("test.cpp", cpp, &options);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let lines: Vec<&str> = rendered.lua.lines().collect();
    assert_eq!(lines.len(), rendered.line_map.len());
    lines
        .iter()
        .zip(&rendered.line_map)
        .skip(6) // the header
        .map(|(text, src)| match src {
            Some(line) => format!("{line:>2} | {text}"),
            None => format!(" - | {text}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn function() {
    let cpp = "\
int x;
/***
 * Gets the health.
 * @function Spring.GetUnitHealth
 * @param unitID integer The unit.
 * @return number health
 */
";
    insta::assert_snapshot!(mapped(cpp));
}

#[test]
fn enum_with_fields_from_other_docs() {
    let cpp = "\
/***
 * Numbers.
 * @enum Numbers
 * @field ONE 1
 */

/*** @field Numbers.TWO 2 Two. */
";
    insta::assert_snapshot!(mapped(cpp));
}

#[test]
fn standalone_field_with_type() {
    let cpp = "\
/***
 * The description.
 * @field Spring.MoveCtrl.Enabled boolean Whether it is on.
 */
";
    insta::assert_snapshot!(mapped(cpp));
}
