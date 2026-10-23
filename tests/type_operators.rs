//! emmylua's type grammar has binary `in`, `+` and `-`, so a description starting with one of
//! them is read as part of the type, silently. v4 reports them as errors.

mod support;

use support::{extract, extract_reporting};

#[test]
fn in_after_a_param_type() {
    let lib = extract_reporting(
        "Spring = {}",
        r#"
        /***
         * @function Spring.SetUnitVelocity
         * @param velX number in elmos/frame
         */
        "#,
    );
    lib.errors(
        3,
        "EmmyLua treats `number in elmos` as an invalid type because `in` is a keyword",
    );
}

#[test]
fn in_after_a_field_type() {
    let lib = extract_reporting(
        "",
        "/*** @field Game.mapSizeX integer in worldspace/opengl coords. */",
    );
    lib.errors(
        1,
        "EmmyLua treats `integer in worldspace` as an invalid type",
    );
}

#[test]
fn in_after_a_return_type() {
    let lib = extract_reporting(
        "Spring = {}",
        r#"
        /***
         * @function Spring.GetTime
         * @return number in seconds
         */
        "#,
    );
    lib.errors(3, "EmmyLua treats `number in seconds` as an invalid type");
}

#[test]
fn the_in_error_suggests_a_fix() {
    let lib = extract_reporting("", "/*** @field Foo.speed number in m/s */");
    lib.errors(1, "@param speed number Speed in m/s.");
}

#[test]
fn minus_after_a_type() {
    let lib = extract_reporting(
        "Spring = {}",
        r#"
        /***
         * @function Spring.GetUnitCount
         * @param teamID integer - the team
         */
        "#,
    );
    lib.errors(3, "EmmyLua treats `integer - the` as an invalid type");
    lib.errors(3, "do not start your description with an `-` or `+`");
}

#[test]
fn plus_after_a_type() {
    let lib = extract_reporting("", "/*** @field Foo.count integer + one more */");
    lib.errors(1, "EmmyLua treats `integer + one` as an invalid type");
}

#[test]
fn in_within_the_description_is_fine() {
    extract(
        r#"
        /***
         * @field Foo.speed number Speed in m/s.
         * @field Foo.range number The range - in elmos.
         */
        "#,
    );
}

#[test]
fn in_a_mapped_type_is_fine() {
    extract(r#"/*** @alias Flags { [K in "a"|"b"]: boolean } */"#);
}
