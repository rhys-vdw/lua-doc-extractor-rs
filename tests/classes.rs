//! Ported from lua-doc-extractor v3's `class.test.ts`.

mod support;

use support::{extract, extract_with};

#[test]
fn class_with_fields() {
    let lib = extract(
        r#"
        /***
         * @class Car
         * @field doorCount integer
         * @field maxSpeed number How fast can this baby go?
         * @field make string The manufacter.
         * @field model string The model of the car.
         */
        "#,
    );
    lib.ty("Car").is("class");
    lib.field("Car.doorCount").has_type("integer");
    lib.field("Car.maxSpeed")
        .has_type("number")
        .has_description("How fast can this baby go?");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn generic_class() {
    let lib = extract(
        r#"
        /***
         * A table of uniform name to value.
         *
         * @class UniformParam<T> : { [string]: T|T[] }
         */
        "#,
    );
    lib.ty("UniformParam")
        .is("class")
        .has_description("A table of uniform name to value.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn several_base_classes_and_a_trailing_description() {
    let lib = extract_with(
        "---@class Bar\n---@class Baz\n---@class A\n---@class B",
        r#"
        /***
         * Here's a description...
         *
         * @class Foo : Bar, Baz, table<string, number>, (A|B) A bit more...
         *
         * Another line!
         */
        "#,
    );
    lib.ty("Foo").is("class");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn complicated_field_types() {
    let lib = extract_with(
        "---@class Bar\n---@class Baz\n---@class Foo<T>",
        r#"
        /***
         * @class Widget
         * @field union Bar | Baz Bar or baz?
         * @field generic Foo<Bar | Baz> A generic.
         */
        "#,
    );
    lib.field("Widget.union")
        .has_type("(Bar|Baz)")
        .has_description("Bar or baz?");
    lib.field("Widget.generic").has_description("A generic.");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn global_class_through_table() {
    let lib = extract(
        r#"
        /***
         * @class Widget
         * @table widget
         * @field a integer
         */

        /***
         * @function widget.Foo
         * @param b integer
         */
        "#,
    );
    lib.function("widget.Foo").param("b").has_type("integer");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn index_style_numeric_fields() {
    let lib = extract(
        r#"
        /***
         * @class ControlPoint
         *
         * NURBS control point.
         *
         * @field [1] number x
         * @field [2] number y
         * @field [3] number z
         * @field [4] number weight
         */
        "#,
    );
    lib.ty("ControlPoint").is("class");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn index_style_string_fields() {
    let lib = extract(
        r#"
        /***
         * @class Example
         *
         * @field ["hello"] "world" World.
         * @field ["foo"] integer A number.
         */
        "#,
    );
    lib.ty("Example").is("class");
    insta::assert_snapshot!(lib.body());
}

/// lua-doc-extractor#79
#[test]
fn index_style_integer_keys() {
    let lib = extract(
        r#"
        /***
         * @class Bad
         * @field n integer
         * @field [integer] string
         */
        "#,
    );
    lib.field("Bad.n").has_type("integer");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn arbitrary_index_style_key_types() {
    let lib = extract_with(
        "---@class CustomKey",
        r#"
        /***
         * @class Example
         * @field [string] number A named key type.
         * @field [ CustomKey ] string A custom key type.
         * @field [intger] boolean An unvalidated key type.
         */
        "#,
    );
    lib.ty("Example").is("class");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn nested_class() {
    let lib = extract_with(
        "Foo = {}",
        r#"
        /***
         * @class Foo.Bar
         *
         * @field a Foo.Bar
         */
        "#,
    );
    lib.ty("Foo.Bar").is("class");
    insta::assert_snapshot!(lib.body());
}

#[test]
fn trailing_whitespace_is_ignored() {
    let lib =
        extract("/***\n * @class Foo\n * @field disabled boolean\n * @field name string \n */");
    lib.field("Foo.name").has_type("string");
    insta::assert_snapshot!(lib.body());
}
