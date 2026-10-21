//! Ported from lua-doc-extractor v3's `enum.test.ts`.

mod support;

use support::{extract, extract_with, extract_with_repo};

#[test]
fn generates_enum_table() {
    let lib = extract(
        r#"
        /***
         * @enum Numbers
         * @field FIVE 5
         * @field SIX 6
         */
        "#,
    );
    lib.ty("Numbers").is("enum");
    lib.field("Numbers.FIVE").has_value("5");
    lib.field("Numbers.SIX").has_value("6");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn enum_description() {
    let lib = extract(
        r#"
        /***
         * @enum Numbers These are numbers.
         * @field FIVE 5
         */
        "#,
    );
    lib.ty("Numbers").has_description("These are numbers.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn merges_enums() {
    let lib = extract(
        r#"
        /***
         * @enum Numbers
         * @field FIVE 5
         */

        /***
         * @enum Numbers These are numbers.
         * @field SIX 6
         */
        "#,
    );
    lib.field("Numbers.FIVE").has_value("5");
    lib.field("Numbers.SIX").has_value("6");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn merges_fields_from_other_docs() {
    let lib = extract(
        r#"
        /***
         * @enum Numbers
         * @field FIVE 5
         */

        /***
         * @field Numbers.SIX 6
         */

        /***
         * @field Numbers.SEVEN 7
         * @field Numbers.EIGHT 8
         */
        "#,
    );
    for (name, value) in [("FIVE", "5"), ("SIX", "6"), ("SEVEN", "7"), ("EIGHT", "8")] {
        lib.field(&format!("Numbers.{name}")).has_value(value);
    }
    insta::assert_snapshot!(lib.body());
}

#[test]
fn field_docs_give_their_description_to_their_fields() {
    let lib = extract(
        r#"
        /***
         * These are numbers.
         * @enum Numbers
         */

        /***
         * This is six.
         * @field Numbers.SIX 6
         */

        /***
         * These are seven and eight.
         * @field Numbers.SEVEN 7 Seven.
         * @field Numbers.EIGHT 8 Eight.
         */
        "#,
    );
    lib.ty("Numbers").has_description("These are numbers.");
    lib.field("Numbers.SIX").has_description("This is six.");
    lib.field("Numbers.SEVEN")
        .has_description("Seven.\n\nThese are seven and eight.");
    lib.field("Numbers.EIGHT")
        .has_description("Eight.\n\nThese are seven and eight.");
    insta::assert_snapshot!(lib.body());
}

/// v3 put an HTML link in the description; v4 adds an `@see` link to each symbol.
#[test]
fn merged_fields_keep_their_own_source_links() {
    let lib = extract_with_repo(
        "",
        r#"
        /***
         * These are numbers.
         * @enum Numbers
         */

        /***
         * @field Numbers.SIX 6 Six.
         */

        /***
         * @field Numbers.SEVEN 7 Seven.
         * @field Numbers.EIGHT 8 Eight.
         */
        "#,
        "https://foo.com",
    );
    insta::assert_snapshot!(lib.body());
}

#[test]
fn negative_literals() {
    let lib = extract(
        r#"
        /***
         * @enum Foo
         * @field Z 0
         * @field A -1
         */

        /***
         * @field Foo.B -2
         */
        "#,
    );
    lib.field("Foo.A").has_value("-1");
    lib.field("Foo.B").has_value("-2");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn nested_enum() {
    let lib = extract_with(
        "Foo = {}",
        r#"
        /***
         * @enum Foo.Bar
         * @field Z 0
         * @field A -1
         */

        /***
         * @field Foo.Bar.B -2
         */
        "#,
    );
    lib.ty("Foo.Bar").is("enum");
    insta::assert_snapshot!(lib.body());
}
