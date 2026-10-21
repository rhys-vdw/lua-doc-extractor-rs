//! Ported from lua-doc-extractor v3's `field.test.ts` and `global.test.ts`.

mod support;

use lua_doc_extractor::doc::Severity;
use support::{extract, extract_reporting, extract_with};

#[test]
fn standalone_field() {
    let lib = extract_with(
        "SomeTable = {}",
        r#"
        /***
         * The description.
         * @field SomeTable.field integer
         */
        "#,
    );
    lib.field("SomeTable.field")
        .has_type("integer")
        .has_description("The description.");
    insta::assert_snapshot!(lib.body());
}

/// The global's inline description runs on over the lines below it, blank lines included.
/// (v3's version of this test used `@field foo`, which v4 no longer accepts as a global.)
#[test]
fn global_with_doc_description() {
    let lib = extract(
        r#"
        /***
         * Some description.
         *
         * @global foo number More.
         *
         * Some more.
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// A global is declared with `@global`. A `@field` needs a table.
#[test]
fn field_without_a_table_is_an_error() {
    let lib = extract_reporting("", "/*** @field foo number A global? */");
    let [d] = lib.diagnostics.as_slice() else {
        panic!("expected one error, found {:#?}", lib.diagnostics);
    };
    assert_eq!(d.severity, Severity::Error, "{d:#?}");
    assert!(d.message.contains("use `@global foo`") || d.message.contains("`@global foo`"));
    assert_eq!(lib.body(), "");
}

#[test]
fn global() {
    let lib = extract(
        r#"
        /***
         * @global foo Foo This is my cool global.
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// `@global` is our tag, so like `@function` its text runs on over the lines below it.
#[test]
fn global_with_description_below() {
    let lib = extract(
        r#"
        /***
         * @global foo Foo
         * This is my cool deprecated global.
         * @deprecated
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// Each type is written to `---@type` as given, and checked by emmylua's parser.
#[test]
fn global_types() {
    let types = [
        "integer",
        "string?",
        "table<string, boolean>",
        "(integer|table<string, boolean>|Foo)?",
        "{}",
        "{ x: integer, y: integer }",
        "{ x: number?, y: A|B }",
        "A|B",
        "integer|string",
        "{ [string]: integer }",
        "{ [(string|table<string, string>?)]: integer }",
        "fun()",
        "fun(x: integer, z: string): string",
        "fun(integer: string): (string|number?)",
        "fun(y: string): string|number?",
        "[number, integer]",
        "integer[]",
        "a.b.c",
    ];
    for ty in types {
        let lib = extract(&format!("/*** @global Foo {ty} The description. */"));
        assert!(
            lib.body().contains(&format!(
                "---The description.\n---\n---@type {ty}\nFoo = nil"
            )),
            "{ty}:\n{}",
            lib.body()
        );
    }
}

/// v3 accepted these. emmylua ends a type at `?` (UPSTREAM.md #13): `T?|U` inside brackets is
/// a parse error, and `T?[]` or `T?|U` alone is silently misread. v4 reports them all.
#[test]
fn global_types_emmylua_rejects() {
    for ty in [
        "(string?|number)?",
        "[table<string>?|integer, nil, true]",
        "(string?|number)[]",
        "string?[]",
        "string?|number",
    ] {
        let lib = extract_reporting("", &format!("/*** @global Foo {ty} The description. */"));
        let [d] = lib.diagnostics.as_slice() else {
            panic!("{ty}: expected one error, found {:#?}", lib.diagnostics);
        };
        assert_eq!(d.severity, Severity::Error, "{ty}: {d:#?}");
        assert!(
            d.message.contains("invalid type in @global"),
            "{ty}: {d:#?}"
        );
    }
}
