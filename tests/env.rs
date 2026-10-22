//! Ported from lua-doc-extractor v3's `env.test.ts`: `@env` routes each doc to an output file
//! by its environments.

mod support;

use lua_doc_extractor::doc::{Diagnostic, Doc, Severity, parse_doc};
use lua_doc_extractor::env::{Routes, apply_file_env, key, parse_routes};
use lua_doc_extractor::{Options, Output, extract_library, scan::scan};
use support::dedent;

/// Three routes over two environments: each one alone, and both.
fn routes() -> Routes {
    let (routes, errors) = parse_routes(&strings(&[
        "server=server.lua",
        "client=client.lua",
        "server,client=common.lua",
    ]));
    assert!(errors.is_empty(), "{errors:?}");
    routes
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

/// Extracts `(path, cpp)` files with `routes`.
fn library(files: &[(&str, &str)], routes: &Routes) -> (Vec<Output>, Vec<Diagnostic>) {
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(path, cpp)| (path.to_string(), dedent(cpp)))
        .collect();
    extract_library(&files, routes, &Options::default())
}

/// Extracts `(path, cpp)` files with `routes`, failing on any diagnostic.
fn outputs(files: &[(&str, &str)], routes: &Routes) -> Vec<Output> {
    let (outputs, diagnostics) = library(files, routes);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    outputs
}

fn names(outputs: &[Output]) -> Vec<&str> {
    let mut names: Vec<&str> = outputs.iter().map(|o| o.name.as_str()).collect();
    names.sort_unstable();
    names
}

fn output<'a>(outputs: &'a [Output], name: &str) -> &'a Output {
    outputs
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no output {name} in {:?}", names(outputs)))
}

/// A file's docs, with its file-level `@env` applied.
fn docs(cpp: &str) -> (Vec<Doc>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut docs: Vec<Doc> = scan(&dedent(cpp))
        .iter()
        .map(|block| parse_doc("a.cpp", block, &mut diagnostics))
        .collect();
    apply_file_env(&mut docs, &mut diagnostics);
    (docs, diagnostics)
}

fn envs(doc: &Doc) -> Vec<String> {
    let mut envs = doc.envs();
    envs.sort();
    envs
}

// key / parse_routes

#[test]
fn key_sorts_trims_and_dedups_the_names() {
    assert_eq!(
        key(&strings(&["client", " server", "server"])),
        "client,server"
    );
    assert_eq!(key(&strings(&["server"])), "server");
}

#[test]
fn routes_are_name_list_equals_file_in_any_order() {
    let (routes, errors) = parse_routes(&strings(&[
        "server=server.lua",
        "client, server=common.lua",
    ]));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(routes.get(&strings(&["server"])), Some("server.lua"));
    assert_eq!(
        routes.get(&strings(&["server", "client"])),
        Some("common.lua")
    );
}

#[test]
fn a_malformed_or_repeated_route_is_an_error() {
    let (routes, errors) = parse_routes(&strings(&[
        "server",
        "=a.lua",
        "server=a.lua",
        "server=b.lua",
    ]));
    assert_eq!(routes.get(&strings(&["server"])), Some("a.lua"));
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(
        errors[0].contains("expected name[,name...]=file"),
        "{errors:?}"
    );
    assert!(errors[2].contains("already routed"), "{errors:?}");
}

// Routing

#[test]
fn a_doc_without_env_goes_to_its_source_file() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @function Game.Print
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["a.cpp.lua"]);
    assert_eq!(outputs[0].sources, ["a.cpp"]);
}

#[test]
fn without_routes_the_tags_change_nothing() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /*** @env server */

            /***
             * @function Game.Save
             */
            "#,
        )],
        &Routes::default(),
    );
    assert_eq!(names(&outputs), ["a.cpp.lua"]);
}

#[test]
fn a_tagged_doc_goes_to_its_envs_file() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @function Game.Save
             * @env server
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["server.lua"]);
}

#[test]
fn a_doc_in_both_environments_goes_to_the_file_for_both_whatever_the_order() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @function Game.Log
             * @env client, server
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["common.lua"]);
}

#[test]
fn the_file_level_env_routes_the_file_and_a_docs_own_env_overrides_it() {
    let outputs = outputs(
        &[(
            "client.cpp",
            r#"
            /*** @env client */

            /***
             * @function Game.GetMouse
             */

            /***
             * @function Game.Log
             * @env server, client
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["client.lua", "common.lua"]);
}

#[test]
fn the_same_env_across_files_merges_into_one_output() {
    let outputs = outputs(
        &[
            (
                "a.cpp",
                r#"
                /***
                 * @function Game.Foo
                 * @env server
                 */
                "#,
            ),
            (
                "b.cpp",
                r#"
                /***
                 * @function Game.Bar
                 * @env server
                 */
                "#,
            ),
        ],
        &routes(),
    );
    assert_eq!(names(&outputs), ["server.lua"]);
    let lua = &outputs[0].rendered.lua;
    assert!(lua.contains("function Game.Foo() end"), "{lua}");
    assert!(lua.contains("function Game.Bar() end"), "{lua}");
    assert_eq!(outputs[0].sources, ["a.cpp", "b.cpp"]);
}

#[test]
fn tagged_and_untagged_docs_from_one_file_part_ways() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @function Game.Foo
             * @env server
             */

            /***
             * @function Game.Bar
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["a.cpp.lua", "server.lua"]);
}

#[test]
fn an_unrouted_env_is_one_error_per_file_and_env() {
    let (outputs, diagnostics) = library(
        &[(
            "a.cpp",
            r#"
            /*** @env menu */

            /***
             * @function Game.Foo
             */

            /***
             * @function Game.Bar
             */
            "#,
        )],
        &routes(),
    );
    assert!(outputs.is_empty(), "{:?}", names(&outputs));
    let [d] = diagnostics.as_slice() else {
        panic!("expected one error: {diagnostics:#?}");
    };
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.path, "a.cpp");
    assert!(d.message.contains("'menu'"), "{d:#?}");
}

#[test]
fn a_constants_enum_routes_by_the_file_level_env() {
    let outputs = outputs(
        &[(
            "keys.cpp",
            r#"
            /*** @env client */

            /***
             * @enum Key
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["client.lua"]);
}

#[test]
fn a_constants_table_in_both_environments_lands_in_the_file_for_both() {
    let outputs = outputs(
        &[(
            "version.cpp",
            r#"
            /*** @env server, client */

            /***
             * @table Version
             */
            "#,
        )],
        &routes(),
    );
    assert_eq!(names(&outputs), ["common.lua"]);
}

// The tag

#[test]
fn env_is_never_written_out() {
    for routes in [routes(), Routes::default()] {
        let outputs = outputs(
            &[(
                "a.cpp",
                r#"
                /***
                 * @function Recoil.Synced.Foo
                 * @env server
                 */
                "#,
            )],
            &routes,
        );
        let lua = &outputs[0].rendered.lua;
        assert!(!lua.contains("@env"), "{lua}");
    }
}

/// v3 kept a continuation line as the tag's description, and dropped it with the tag.
#[test]
fn env_names_and_a_continuation_line_that_is_never_written() {
    let (docs, diagnostics) = docs(
        r#"
        /***
         * @function Foo.Bar
         * @env server, client
         * only on the two
         */
        "#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(docs[0].envs(), ["server", "client"]);
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @function Foo.Bar
             * @env server, client
             * only on the two
             */
            "#,
        )],
        &Routes::default(),
    );
    let lua = &outputs[0].rendered.lua;
    assert!(!lua.contains("only on the two"), "{lua}");
}

#[test]
fn a_single_env() {
    let (docs, _) = docs(
        r#"
        /***
         * @function Foo.Bar
         * @env server
         */
        "#,
    );
    assert_eq!(envs(&docs[0]), ["server"]);
}

#[test]
fn comma_separated_envs() {
    let (docs, _) = docs(
        r#"
        /***
         * @function Foo.Bar
         * @env server, client
         */
        "#,
    );
    assert_eq!(envs(&docs[0]), ["client", "server"]);
}

#[test]
fn no_env_is_empty() {
    let (docs, _) = docs(
        r#"
        /***
         * @function Foo.Bar
         */
        "#,
    );
    assert!(docs[0].envs().is_empty());
}

// File-level markers

#[test]
fn a_file_level_env_applies_to_the_docs_below_and_is_removed() {
    let (docs, diagnostics) = docs(
        r#"
        /*** @env server */

        /***
         * @function Game.Attach
         */

        /***
         * @function Game.Drop
         */
        "#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(docs.len(), 2, "the marker is removed");
    assert_eq!(envs(&docs[0]), ["server"]);
    assert_eq!(envs(&docs[1]), ["server"]);
}

#[test]
fn a_docs_own_env_is_kept() {
    let (docs, _) = docs(
        r#"
        /*** @env server */

        /***
         * @function Foo.Bar
         * @env client
         */
        "#,
    );
    assert_eq!(envs(&docs[0]), ["client"]);
}

#[test]
fn no_file_level_env_changes_nothing() {
    let (docs, diagnostics) = docs(
        r#"
        /***
         * @function Foo.Bar
         */
        "#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(docs.len(), 1);
    assert!(docs[0].envs().is_empty());
}

#[test]
fn a_file_level_env_after_another_doc_is_an_error() {
    let (_, diagnostics) = docs(
        r#"
        /***
         * @function Foo.Bar
         */

        /*** @env server */
        "#,
    );
    let [d] = diagnostics.as_slice() else {
        panic!("expected one error: {diagnostics:#?}");
    };
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("must be the first doc"), "{d:#?}");
}

#[test]
fn more_than_one_file_level_env_is_an_error() {
    let (_, diagnostics) = docs(
        r#"
        /*** @env server */

        /*** @env client */

        /***
         * @function Foo.Bar
         */
        "#,
    );
    let [d] = diagnostics.as_slice() else {
        panic!("expected one error: {diagnostics:#?}");
    };
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.pos.line, 3, "{d:#?}");
    assert!(
        d.message.contains("more than one file-level @env"),
        "{d:#?}"
    );
}

// New in v4

/// A marker declares nothing, but it isn't a doc with only a description, so it doesn't warn.
#[test]
fn a_file_level_env_is_not_reported_as_unattached() {
    for routes in [routes(), Routes::default()] {
        let (_, diagnostics) = library(
            &[(
                "a.cpp",
                r#"
                /*** @env server */

                /***
                 * @function Game.Save
                 */
                "#,
            )],
            &routes,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }
}

/// Docs are routed one at a time, and tables merge within each output, so a table can be
/// partly in one environment and partly in another.
#[test]
fn a_table_split_across_environments() {
    let outputs = outputs(
        &[
            (
                "synced.cpp",
                r#"
                /*** @env server */

                /***
                 * The table.
                 * @table Spring
                 */

                /***
                 * @function Spring.Save
                 */
                "#,
            ),
            (
                "unsynced.cpp",
                r#"
                /*** @env client */

                /***
                 * @table Spring
                 */

                /***
                 * @function Spring.GetMouse
                 */
                "#,
            ),
        ],
        &routes(),
    );
    assert_eq!(names(&outputs), ["client.lua", "server.lua"]);
    let server = &output(&outputs, "server.lua").rendered.lua;
    let client = &output(&outputs, "client.lua").rendered.lua;
    assert!(
        server.contains("Spring = {}") && server.contains("function Spring.Save() end"),
        "{server}"
    );
    assert!(!server.contains("GetMouse"), "{server}");
    assert!(
        client.contains("Spring = {}") && client.contains("function Spring.GetMouse() end"),
        "{client}"
    );
    assert!(!client.contains("Save"), "{client}");
}

/// A field routed away from its table's doc is written on its own, as `Table.name = value`.
#[test]
fn a_field_in_another_environment_from_its_table() {
    let outputs = outputs(
        &[(
            "a.cpp",
            r#"
            /***
             * @table Game
             * @env server, client
             */

            /***
             * @field Game.speed integer
             * @env server
             */
            "#,
        )],
        &routes(),
    );
    let common = &output(&outputs, "common.lua").rendered.lua;
    let server = &output(&outputs, "server.lua").rendered.lua;
    assert!(common.contains("Game = {}"), "{common}");
    assert!(server.contains("Game.speed = nil"), "{server}");
}

#[test]
fn an_env_name_must_be_a_name() {
    let (_, diagnostics) = docs(
        r#"
        /***
         * @function Foo.Bar
         * @env server client
         */
        "#,
    );
    let [d] = diagnostics.as_slice() else {
        panic!("expected one error: {diagnostics:#?}");
    };
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("@env"), "{d:#?}");
}
