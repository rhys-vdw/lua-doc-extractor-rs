//! `remap`: a checker's diagnostics on the generated library, moved onto the C++.

use std::path::{Path, PathBuf};

use lua_doc_extractor::remap::{
    CheckDiagnostic, Level, Remapped, SourceMap, annotation, github, read_check, remap, text,
    uri_to_path,
};
use serde_json::json;

#[test]
fn file_uris_become_paths() {
    assert_eq!(
        uri_to_path("file:///c%3A/Users/a%20b/x.lua"),
        PathBuf::from("c:/Users/a b/x.lua")
    );
    assert_eq!(
        uri_to_path("file:///home/x.lua"),
        PathBuf::from("/home/x.lua")
    );
    assert_eq!(uri_to_path("rts/x.lua"), PathBuf::from("rts/x.lua"));
}

fn diagnostic(severity: u64, line: u64, code: serde_json::Value) -> serde_json::Value {
    json!({
        "range": { "start": { "line": line, "character": 4 }, "end": { "line": line, "character": 9 } },
        "severity": severity,
        "code": code,
        "message": "Something.",
    })
}

#[test]
fn reads_luals_check_json() {
    let check = json!({
        "file:///c%3A/lib/a.cpp.lua": [diagnostic(2, 9, json!("undefined-doc-name"))],
    });
    let diagnostics = read_check(&check).unwrap();
    assert_eq!(
        diagnostics,
        [CheckDiagnostic {
            file: PathBuf::from("c:/lib/a.cpp.lua"),
            line: 9,
            level: Level::Warning,
            code: Some("undefined-doc-name".into()),
            message: "Something.".into(),
        }]
    );
}

#[test]
fn reads_emmylua_check_json_without_hints() {
    let check = json!([{
        "file": "C:/lib/a.cpp.lua",
        "diagnostics": [
            diagnostic(1, 3, json!("syntax-error")),
            diagnostic(4, 4, json!("unused")),
            diagnostic(3, 5, json!(7)),
        ],
    }]);
    let diagnostics = read_check(&check).unwrap();
    let summary: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.line, d.level, d.code.as_deref()))
        .collect();
    assert_eq!(
        summary,
        [
            (3, Level::Error, Some("syntax-error")),
            (5, Level::Notice, Some("7")),
        ]
    );
}

#[test]
fn rejects_other_json() {
    assert!(read_check(&json!("nope")).is_err());
    assert!(read_check(&json!([{ "nope": 1 }])).is_err());
}

#[test]
fn rejects_a_version_1_source_map() {
    let map = json!({ "version": 1, "files": {} });
    assert!(SourceMap::from_json(&map).is_err());
}

/// A working directory with a generated library under `lib/generated`, a hand-written file
/// `lib/types.lua`, and a source map for `lib/generated/rts/a.cpp.lua`.
struct Workspace {
    _dir: tempfile::TempDir,
    root: PathBuf,
    map: SourceMap,
}

impl Workspace {
    fn new() -> Workspace {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("lib/generated/rts")).unwrap();
        std::fs::write(root.join("lib/generated/rts/a.cpp.lua"), "x\n").unwrap();
        std::fs::write(root.join("lib/types.lua"), "x\n").unwrap();
        let map = SourceMap::from_json(&json!({
            "version": 2,
            "files": {
                "rts/a.cpp.lua": {
                    "sources": ["rts/a.cpp", "rts/b.cpp"],
                    "lines": [null, [0, 12], [1, 40]],
                },
            },
        }))
        .unwrap();
        Workspace {
            _dir: dir,
            root,
            map,
        }
    }

    fn remap(&self, file: &Path, line: usize) -> Remapped {
        let d = CheckDiagnostic {
            file: file.to_path_buf(),
            line,
            level: Level::Error,
            code: Some("code".into()),
            message: "Message.".into(),
        };
        let [r] = remap(
            &[d],
            &self.map,
            &self.root.join("lib/generated"),
            &self.root,
        )
        .try_into()
        .unwrap();
        r
    }
}

fn place(r: &Remapped) -> (&str, usize) {
    (r.path.as_str(), r.line)
}

#[test]
fn a_generated_line_moves_to_its_cpp_line() {
    let ws = Workspace::new();
    let generated = ws.root.join("lib/generated/rts/a.cpp.lua");
    assert_eq!(place(&ws.remap(&generated, 1)), ("rts/a.cpp", 12));
    // An output can mix sources.
    assert_eq!(place(&ws.remap(&generated, 2)), ("rts/b.cpp", 40));
}

/// The header and blank lines have no C++ line, so they're reported in the generated file.
#[test]
fn a_line_with_no_source_stays_in_the_generated_file() {
    let ws = Workspace::new();
    let generated = ws.root.join("lib/generated/rts/a.cpp.lua");
    assert_eq!(
        place(&ws.remap(&generated, 0)),
        ("lib/generated/rts/a.cpp.lua", 1)
    );
}

#[test]
fn a_file_that_isnt_generated_is_reported_where_it_is() {
    let ws = Workspace::new();
    assert_eq!(
        place(&ws.remap(&ws.root.join("lib/types.lua"), 4)),
        ("lib/types.lua", 5)
    );
}

#[test]
fn formats() {
    let r = Remapped {
        path: "rts/a,b.cpp".into(),
        line: 12,
        level: Level::Warning,
        code: Some("type-not-found".into()),
        message: "Type 'x' not found\n100%".into(),
    };
    assert_eq!(
        github(&r),
        "::warning file=rts/a%2Cb.cpp,line=12,title=type-not-found::Type 'x' not found%0A100%25"
    );
    assert_eq!(
        text(&r),
        "rts/a,b.cpp:12: warning: Type 'x' not found\n100% [type-not-found]"
    );
}

/// The extractor's own diagnostics have a column, and no code.
#[test]
fn an_annotation_with_a_column() {
    assert_eq!(
        annotation(
            Level::Error,
            "rts/a.cpp",
            3,
            Some(4),
            None,
            "Invalid `@param` name `fun(...)`."
        ),
        "::error file=rts/a.cpp,line=3,col=4::Invalid `@param` name `fun(...)`."
    );
}
