//! Ported from lua-doc-extractor v3's `table.test.ts` and `mergeTables.test.ts`.

mod support;

use support::{extract, extract_reporting, extract_with};

#[test]
fn table_with_fields() {
    let lib = extract(
        r#"
        /***
         * My table.
         * @field foo integer This is foo.
         * @field bar string This is bar.
         * @table TheTable
         */
        "#,
    );
    lib.field("TheTable.foo")
        .has_type("integer")
        .has_description("This is foo.");
    lib.field("TheTable.bar")
        .has_type("string")
        .has_description("This is bar.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn empty_table() {
    let lib = extract(
        r#"
        /***
         * My empty table.
         * @table Empty
         */
        "#,
    );
    insta::assert_snapshot!(lib.body());
}

/// v3 accepted descriptions on the lines below each field. emmylua gives such text to the
/// next tag, so v4 warns, and the text belongs to the field below it.
#[test]
fn field_description_below_the_field_warns() {
    let lib = extract_reporting(
        "",
        r#"
        /***
         * My table.
         * @field foo integer
         * This is foo.
         *
         * @field bar string Bar.
         * @table TheTable
         */
        "#,
    );
    lib.warns(
        3,
        "Text below `@field`. Put the description on the tag line, or for a longer one, in its own doc comment above `@field foo integer`.",
    );
    lib.field("TheTable.bar")
        .has_description("This is foo.\n\nBar.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn keyword_field_names_are_quoted() {
    let lib = extract(
        r#"
        /***
         * Cool table.
         * @field value integer The value.
         * @field repeat boolean Should this repeat?
         * @table CoolTable
         */
        "#,
    );
    lib.field("CoolTable.repeat").has_type("boolean");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn nested_table() {
    let lib = extract_with("Spring = {}", "/*** @table Spring.MoveCtrl */");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn merges_tables() {
    let lib = extract(
        r#"
        /***
         * Foo description.
         * @table Foo
         * @field foo integer This is foo.
         */

        /***
         * Additional.
         * @table Foo
         * @field bar string This is bar.
         */
        "#,
    );
    lib.field("Foo.foo").has_type("integer");
    lib.field("Foo.bar").has_type("string");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn merges_a_class_with_a_table_of_the_same_name() {
    let lib = extract(
        r#"
        /***
         * Foo description.
         * @class Foo First.
         * @field foo integer This is foo.
         */

        /***
         * Additional.
         * @table Foo Second.
         * @field bar string This is bar.
         */
        "#,
    );
    lib.field("Foo.foo")
        .has_type("integer")
        .has_description("This is foo.");
    lib.field("Foo.bar")
        .has_type("string")
        .has_description("This is bar.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn merges_an_enum_with_a_table_of_the_same_name() {
    let lib = extract(
        r#"
        /***
         * Foo description.
         * @enum Foo First.
         * @field FIVE 5
         */

        /***
         * Additional.
         * @table Foo Second.
         * @field SIX 6
         */
        "#,
    );
    lib.field("Foo.FIVE").has_value("5");
    lib.field("Foo.SIX").has_value("6");
    insta::assert_snapshot!(lib.body());
}

/// A table-typed field gets `{}`; any other type gets `nil`, which a checker doesn't compare with
/// the `---@type` of a field in a table constructor.
#[test]
fn field_placeholder_values() {
    let lib = extract(
        r#"
        /***
         * @field list integer[] A list.
         * @field count integer A count.
         * @table TheTable
         */
        "#,
    );
    let body = lib.body();
    assert!(body.contains("---@type integer[]\n\tlist = {},"), "{body}");
    assert!(body.contains("---@type integer\n\tcount = nil\n"), "{body}");
    assert!(!body.contains("@diagnostic"), "{body}");
}
