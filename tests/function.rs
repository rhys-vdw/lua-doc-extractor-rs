//! Ported from lua-doc-extractor v3's `function.test.ts`, checked against emmylua's view.

mod support;

use support::{extract, extract_with};

#[test]
fn description_before_and_after_function_tag() {
    let lib = extract_with(
        "SomeTable = {}",
        r#"
        /*** Called when a unit emits a seismic ping.
         *
         * @function SomeTable.UnitSeismicPing
         *
         * See `seismicSignature`.
         *
         * @param x number
         * @param y number
         * @param z number
         */
        "#,
    );
    let f = lib.function("SomeTable.UnitSeismicPing");
    f.has_description("Called when a unit emits a seismic ping.\n\nSee `seismicSignature`.");
    f.param_names(&["x", "y", "z"]);
    f.param("x").has_type("number");
}

#[test]
fn generic_function() {
    let lib = extract_with(
        "SomeTable = {}",
        r#"
        /***
         * Does foo.
         *
         * @function SomeTable.Foo
         *
         * @generic T : integer
         * @param x T
         * @param y integer
         */
        "#,
    );
    let f = lib.function("SomeTable.Foo");
    f.has_description("Does foo.");
    f.param("x").has_type("T");
    f.param("y").has_type("integer");
}

#[test]
fn method() {
    let lib = extract_with(
        "---@class SomeTable\nSomeTable = {}",
        r#"
        /***
         * Does foo.
         *
         * @function SomeTable:Foo
         * @param x integer
         */
        "#,
    );
    lib.function("SomeTable:Foo").param("x").has_type("integer");
}

#[test]
fn variadic_param() {
    let lib = extract_with(
        "SomeTable = {}",
        r#"
        /***
         * @function SomeTable.variadic
         * @param name string
         * @param ... integer
         */
        "#,
    );
    let f = lib.function("SomeTable.variadic");
    f.param_names(&["name", "..."]);
    f.param("...").has_type("integer");
}

#[test]
fn multiple_returns_with_literal_keywords() {
    let lib = extract_with(
        "Spring = {}",
        r#"
        /***
         * @function Spring.GetUnitMoveDefID
         *
         * Returns a numerical movedef ID and its name.
         *
         * @param unitID integer
         *
         * @return integer|false|nil moveDefID Some long description
         * @return string|nil moveDefName Another really, really long description
         */
        "#,
    );
    let f = lib.function("Spring.GetUnitMoveDefID");
    f.has_description("Returns a numerical movedef ID and its name.");
    f.returns(0).named("moveDefID").has_type("(integer|false)?").has_description("Some long description");
    f.returns(1).named("moveDefName").has_type("string?").has_description("Another really, really long description");
}

#[test]
fn several_function_tags_share_one_doc() {
    let lib = extract_with(
        "Spring = {}",
        r#"
        /***
         * Gets a value.
         * @function Spring.GetA
         * @function Spring.GetB
         * @return number value
         */
        "#,
    );
    for name in ["Spring.GetA", "Spring.GetB"] {
        let f = lib.function(name);
        f.has_description("Gets a value.");
        f.returns(0).named("value").has_type("number");
    }
}

/// emmylua's doc JSON has no params for global functions (UPSTREAM.md), so this only
/// checks that the function exists.
#[test]
fn global_function() {
    let lib = extract(
        r#"
        /***
         * @function UnitSeismicPing
         * @param x number The x.
         */
        "#,
    );
    assert!(lib.lua.contains("function UnitSeismicPing(x) end"), "{}", lib.lua);
}
