//! Ported from lua-doc-extractor v3's `comments.test.ts`, `ldoc.test.ts`,
//! `preserveIndentation.test.ts` and `sourceLinks.test.ts`.

mod support;

use lua_doc_extractor::doc::Severity;
use support::{extract, extract_reporting, extract_with, extract_with_repo};

#[test]
fn only_triple_star_comments_are_read() {
    let lib = extract(
        r#"
        /***
         * @table Foo
         */

        /**
         * @table Bar
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

#[test]
fn single_line_tag() {
    let lib = extract("/*** @table Foo */");
    insta::assert_snapshot!(lib.body());
}

/// A tag with nothing to generate is kept as a comment.
#[test]
fn single_line_tag_without_code() {
    let lib = extract("/*** @deprecated */");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn extra_stars_after_the_opening_are_ignored() {
    let lib = extract("/****************\n * @table Foo\n */");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn unknown_tag_followed_by_text() {
    let lib = extract(
        r#"
        /***
         * @usage
         * another line
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

#[test]
fn inline_code_with_double_quotes() {
    let lib = extract_with(
        "Spring = {}",
        r#"
        /***
         * @function Spring.SendMessage
         * @param message string ``"`<PLAYER#>`"`` where `#` is a player ID.
         */
        "#,
    );
    lib.function("Spring.SendMessage")
        .param("message")
        .has_description(r#"``"`<PLAYER#>`"`` where `#` is a player ID."#);
}

#[test]
fn empty_comments_are_ignored() {
    let lib = extract("/***\n *\n */");
    assert_eq!(lib.body(), "\n");
}

/// v3 accepted LDoc's `@return?`. emmylua can't parse it, so v4 reports it, and passes the
/// tag through unchanged.
#[test]
fn ldoc_optional_return_is_an_error() {
    let lib = extract_reporting(
        "LDocTable = {}",
        r#"
        /***
         * @function LDocTable.LDoc
         * @param s string
         * @return? boolean
         */
        "#,
    );
    let [d] = lib.diagnostics.as_slice() else {
        panic!("expected one error, found {:#?}", lib.diagnostics);
    };
    assert_eq!((d.severity, d.pos.line), (Severity::Error, 4), "{d:#?}");
    assert!(d.message.starts_with("in @return?: "), "{d:#?}");
    lib.function("LDocTable.LDoc").param("s").has_type("string");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn indentation_in_description_is_kept() {
    let lib = extract(
        r#"
        /***
         * This is a list:
         * - Foo
         *   - Bar
         *     - Baz
         * @table List
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// v3 kept multi-line param descriptions. v4 requires them on one line, and warns.
#[test]
fn multi_line_param_description_warns() {
    let lib = extract_reporting(
        "gl = {}",
        r#"
        /***
         * @function gl.Text
         * @param options string? concatenated string of option characters.
         *
         *   - horizontal alignment:
         *     - 'c' = center
         */
        "#,
    );
    lib.warns(3, "@param description must be on one line");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn indentation_in_code_block_is_kept() {
    let lib = extract(
        r#"
        /***
         * ```lua
         * for i = 1, #teams do
         *   if teams[i] ~= teamID and not select(2, Spring.GetTeamInfo(teams[i], false)) then
         *   aliveTeams = aliveTeams + 1
         * end
         * ```
         * @function Foo
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// v3 put an HTML link in the description; v4 adds an `@see` link to each symbol.
#[test]
fn source_links() {
    let lib = extract_with_repo(
        "",
        r#"
        /***
         * @table MyTable
         */

        /***
         * @function MyTable.Boop
         * @param x number
         */
        "#,
        "https://github.com/rhys-vdw/lua-doc-extractor/blob/main/",
    );
    insta::assert_snapshot!(lib.body());
}

#[test]
fn no_source_links_when_nothing_is_generated() {
    let lib = extract_with_repo(
        "",
        "/*************************************/\n\n/*************************************/",
        "https://github.com/rhys-vdw/lua-doc-extractor/blob/main/",
    );
    assert_eq!(lib.body(), "\n");
}
