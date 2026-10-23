//! Each generated line maps to the C++ line it came from.

use lua_doc_extractor::{Options, extract};

/// The generated body, each line prefixed with its C++ line number (or `-`).
fn mapped(cpp: &str) -> String {
    let options = Options {
        repo: Some("https://example.com/".into()),
        ..Default::default()
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
            Some(src) => format!("{:>2} | {text}", src.line),
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

/// The JSON source map, for outputs from several files.
mod source_map {
    use lua_doc_extractor::env::parse_routes;
    use lua_doc_extractor::{Options, extract_library, source_map};
    use serde_json::{Value, json};

    /// Extracts `(path, cpp)` files, routing `@env server` to `server.lua`.
    fn map(files: &[(&str, &str)]) -> (Vec<lua_doc_extractor::Output>, Value) {
        let (routes, errors) = parse_routes(&["server=server.lua".to_string()]);
        assert!(errors.is_empty());
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        let (outputs, diagnostics) = extract_library(&files, &routes, &Options::default());
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let map = source_map(&outputs);
        (outputs, map)
    }

    /// The `[source, line]` of the output line that is exactly `text`.
    fn origin(outputs: &[lua_doc_extractor::Output], map: &Value, name: &str, text: &str) -> Value {
        let output = outputs.iter().find(|o| o.name == name).unwrap();
        let i = output
            .rendered
            .lua
            .lines()
            .position(|l| l == text)
            .unwrap_or_else(|| panic!("no line {text:?} in:\n{}", output.rendered.lua));
        let file = &map["files"][name];
        let [source, line] = file["lines"][i].as_array().unwrap().as_slice() else {
            panic!("{text:?} has no source: {}", file["lines"][i]);
        };
        json!([file["sources"][source.as_u64().unwrap() as usize], line])
    }

    #[test]
    fn one_source() {
        let (outputs, map) = map(&[("a.cpp", "\n/***\n * @function Spring.A\n */\n")]);
        assert_eq!(map["version"], 2);
        let file = &map["files"]["a.cpp.lua"];
        assert_eq!(file["sources"], json!(["a.cpp"]));
        assert_eq!(
            file["lines"].as_array().unwrap().len(),
            outputs[0].rendered.lua.lines().count()
        );
        assert_eq!(
            origin(&outputs, &map, "a.cpp.lua", "function Spring.A() end"),
            json!(["a.cpp", 3])
        );
        // The header is generated.
        assert_eq!(file["lines"][0], Value::Null);
    }

    /// One output from two files, with a table whose comment merges docs from both: each line
    /// maps to the file it came from.
    #[test]
    fn an_output_from_several_files() {
        let a = "/*** @env server */\n\n/***\n * From A.\n * @table Spring\n */\n\n/*** @function Spring.A */\n";
        let b = "/*** @env server */\n\n/***\n * From B.\n * @table Spring\n */\n\n/*** @function Spring.B */\n";
        let (outputs, map) = map(&[("a.cpp", a), ("b.cpp", b)]);
        assert_eq!(
            map["files"]["server.lua"]["sources"],
            json!(["a.cpp", "b.cpp"])
        );
        assert_eq!(
            origin(&outputs, &map, "server.lua", "---From A."),
            json!(["a.cpp", 4])
        );
        assert_eq!(
            origin(&outputs, &map, "server.lua", "---From B."),
            json!(["b.cpp", 4])
        );
        assert_eq!(
            origin(&outputs, &map, "server.lua", "function Spring.A() end"),
            json!(["a.cpp", 8])
        );
        assert_eq!(
            origin(&outputs, &map, "server.lua", "function Spring.B() end"),
            json!(["b.cpp", 8])
        );
    }
}
