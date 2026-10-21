//! The test oracle: runs the extractor, then asks emmylua what it understood.
//!
//! Tests state facts about emmylua's view, one per line:
//!
//! ```ignore
//! let lib = extract(cpp);
//! let f = lib.function("Spring.GetUnitHealth");
//! f.param("unitID").has_type("integer").has_description("The unit.");
//! ```
//!
//! A failed fact prints emmylua's view of the symbol.

#![allow(dead_code)]

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};
use lua_doc_extractor::doc::{Diagnostic, Severity};
use lua_doc_extractor::{Options, extract as extract_lua};
use serde_json::Value;

/// emmylua's understanding of a generated library.
pub struct Lib {
    /// The generated Lua, for failure messages.
    pub lua: String,
    pub diagnostics: Vec<Diagnostic>,
    doc: Value,
}

/// Extracts `cpp` as `test.cpp`, and analyses the output with emmylua. `stubs` is Lua that
/// declares the tables the docs use, e.g. `Spring = {}`. Any diagnostic fails the test.
pub fn extract_with(stubs: &str, cpp: &str) -> Lib {
    let lib = run(stubs, cpp, &Options::default());
    assert!(
        lib.diagnostics.is_empty(),
        "unexpected diagnostics: {:#?}",
        lib.diagnostics
    );
    lib
}

/// Extracts `cpp` with no stubs. Any diagnostic fails the test.
pub fn extract(cpp: &str) -> Lib {
    extract_with("", cpp)
}

/// Extracts `cpp` with source links to `repo`. Any diagnostic fails the test.
pub fn extract_with_repo(stubs: &str, cpp: &str, repo: &str) -> Lib {
    let options = Options {
        repo: Some(repo.to_string()),
    };
    let lib = run(stubs, cpp, &options);
    assert!(
        lib.diagnostics.is_empty(),
        "unexpected diagnostics: {:#?}",
        lib.diagnostics
    );
    lib
}

/// Extracts `cpp`, keeping its diagnostics for the test to check.
pub fn extract_reporting(stubs: &str, cpp: &str) -> Lib {
    run(stubs, cpp, &Options::default())
}

fn run(stubs: &str, cpp: &str, options: &Options) -> Lib {
    let (lua, diagnostics) = extract_lua("test.cpp", &dedent(cpp), options);

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("stubs.lua"), format!("---@meta\n{stubs}\n")).unwrap();
    std::fs::write(dir.path().join("test.cpp.lua"), &lua).unwrap();
    let json = dir.path().join("doc.json");
    run_doc_cli(CmdArgs {
        config: None,
        workspace: vec![dir.path().to_path_buf()],
        exclude_pattern: None,
        include_pattern: None,
        output_format: Format::Json,
        output: OutputDestination::File(json.clone()),
        override_template: None,
        site_name: None,
        mixin: None,
        verbose: false,
    })
    .expect("emmylua_doc_cli failed");
    let doc = serde_json::from_str(&std::fs::read_to_string(json).unwrap()).unwrap();
    Lib {
        lua,
        diagnostics,
        doc,
    }
}

impl Lib {
    /// The generated Lua after the header, for snapshots.
    pub fn body(&self) -> &str {
        self.lua
            .split_once("-- ------------\n\n")
            .map_or(&self.lua, |(_, body)| body)
    }

    /// Asserts exactly one warning, at `line` of the C++, containing `text`.
    pub fn warns(&self, line: usize, text: &str) {
        let [d] = self.diagnostics.as_slice() else {
            panic!("expected one warning, found {:#?}", self.diagnostics);
        };
        assert_eq!(d.severity, Severity::Warning, "{d:#?}");
        assert_eq!(d.pos.line, line, "{d:#?}");
        assert!(d.message.contains(text), "{d:#?}");
    }

    /// A function on a table or class: `Spring.GetUnitHealth`, `Foo:bar`.
    pub fn function(&self, name: &str) -> Function<'_> {
        let (owner, member) = name.rsplit_once(['.', ':']).expect("a function on a table");
        let value = self
            .members(owner)
            .find(|m| m["type"] == "fn" && m["name"] == member)
            .unwrap_or_else(|| self.fail(&format!("no function {name}"), self.owner(owner)));
        Function {
            lib: self,
            name: name.to_string(),
            value,
        }
    }

    /// A field on a table or class: `CMD.STOP`, `ArchiveInfo.name`.
    pub fn field(&self, name: &str) -> Field<'_> {
        let (owner, member) = name.rsplit_once('.').expect("a field on a table");
        let value = self
            .members(owner)
            .find(|m| m["type"] == "field" && m["name"] == member)
            .unwrap_or_else(|| self.fail(&format!("no field {name}"), self.owner(owner)));
        Field {
            lib: self,
            name: name.to_string(),
            value,
        }
    }

    /// A class, enum or alias.
    pub fn ty(&self, name: &str) -> Type<'_> {
        let value = self
            .types()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| self.fail(&format!("no type {name}"), None));
        Type {
            lib: self,
            name: name.to_string(),
            value,
        }
    }

    fn types(&self) -> impl Iterator<Item = &Value> {
        self.doc["types"].as_array().into_iter().flatten()
    }

    /// The table or type named `name` that holds members. A global typed as a class
    /// (`---@class Foo` + `foo = {}`) has its members on the class.
    fn owner(&self, name: &str) -> Option<&Value> {
        let globals = self.doc["globals"].as_array().into_iter().flatten();
        let mut candidates = globals.chain(self.types()).filter(|v| v["name"] == name);
        let first = candidates.next()?;
        if first["members"].is_array() {
            return Some(first);
        }
        if let Some(other) = candidates.next() {
            return Some(other);
        }
        let class = first["typ"]
            .as_str()
            .and_then(|t| self.types().find(|v| v["name"] == t));
        class.or(Some(first))
    }

    fn members(&self, owner: &str) -> impl Iterator<Item = &Value> {
        let owner = self
            .owner(owner)
            .unwrap_or_else(|| self.fail(&format!("no table or type {owner}"), None));
        owner["members"].as_array().into_iter().flatten()
    }

    fn fail(&self, message: &str, view: Option<&Value>) -> ! {
        let view = view.map_or("(nothing)".to_string(), |v| {
            serde_json::to_string_pretty(&strip(v)).unwrap()
        });
        panic!(
            "{message}\n\nemmylua's view:\n{view}\n\ngenerated Lua:\n{}",
            self.lua
        );
    }
}

pub struct Function<'a> {
    lib: &'a Lib,
    name: String,
    value: &'a Value,
}

impl<'a> Function<'a> {
    pub fn has_description(&self, expected: &str) -> &Self {
        self.check("description", text(&self.value["description"]), expected);
        self
    }

    pub fn param(&self, name: &str) -> Param<'a> {
        let value = self.value["params"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| {
                self.lib.fail(
                    &format!("{} has no param {name}", self.name),
                    Some(self.value),
                )
            });
        Param {
            lib: self.lib,
            subject: format!("{} param {name}", self.name),
            owner: self.value,
            value,
        }
    }

    /// The `index`th return value, from 0.
    pub fn returns(&self, index: usize) -> Param<'a> {
        let value = self.value["returns"].get(index).unwrap_or_else(|| {
            self.lib.fail(
                &format!("{} has no return {index}", self.name),
                Some(self.value),
            )
        });
        Param {
            lib: self.lib,
            subject: format!("{} return {index}", self.name),
            owner: self.value,
            value,
        }
    }

    pub fn param_names(&self, expected: &[&str]) -> &Self {
        let names: Vec<&str> = self.value["params"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["name"].as_str())
            .collect();
        if names != expected {
            self.lib.fail(
                &format!(
                    "{} params: expected {expected:?}, found {names:?}",
                    self.name
                ),
                Some(self.value),
            );
        }
        self
    }

    fn check(&self, what: &str, actual: String, expected: &str) {
        if actual != expected {
            self.lib.fail(
                &format!(
                    "{} {what}: expected {expected:?}, found {actual:?}",
                    self.name
                ),
                Some(self.value),
            );
        }
    }
}

/// A param or return value.
pub struct Param<'a> {
    lib: &'a Lib,
    subject: String,
    owner: &'a Value,
    value: &'a Value,
}

impl Param<'_> {
    pub fn has_type(&self, expected: &str) -> &Self {
        self.check(
            "type",
            self.value["typ"].as_str().unwrap_or("").to_string(),
            expected,
        )
    }

    pub fn named(&self, expected: &str) -> &Self {
        self.check(
            "name",
            self.value["name"].as_str().unwrap_or("").to_string(),
            expected,
        )
    }

    pub fn has_description(&self, expected: &str) -> &Self {
        self.check("description", text(&self.value["desc"]), expected)
    }

    fn check(&self, what: &str, actual: String, expected: &str) -> &Self {
        if actual != expected {
            self.lib.fail(
                &format!(
                    "{} {what}: expected {expected:?}, found {actual:?}",
                    self.subject
                ),
                Some(self.owner),
            );
        }
        self
    }
}

pub struct Field<'a> {
    lib: &'a Lib,
    name: String,
    value: &'a Value,
}

impl Field<'_> {
    pub fn has_type(&self, expected: &str) -> &Self {
        self.check(
            "type",
            self.value["typ"].as_str().unwrap_or("").to_string(),
            expected,
        )
    }

    /// The literal value, e.g. `0` for `CMD.STOP = 0`.
    pub fn has_value(&self, expected: &str) -> &Self {
        self.check(
            "value",
            self.value["literal"].as_str().unwrap_or("").to_string(),
            expected,
        )
    }

    pub fn has_description(&self, expected: &str) -> &Self {
        self.check("description", text(&self.value["description"]), expected)
    }

    fn check(&self, what: &str, actual: String, expected: &str) -> &Self {
        if actual != expected {
            self.lib.fail(
                &format!(
                    "{} {what}: expected {expected:?}, found {actual:?}",
                    self.name
                ),
                Some(self.value),
            );
        }
        self
    }
}

pub struct Type<'a> {
    lib: &'a Lib,
    name: String,
    value: &'a Value,
}

impl Type<'_> {
    pub fn is(&self, kind: &str) -> &Self {
        self.check(
            "kind",
            self.value["type"].as_str().unwrap_or("").to_string(),
            kind,
        )
    }

    pub fn has_description(&self, expected: &str) -> &Self {
        self.check("description", text(&self.value["description"]), expected)
    }

    fn check(&self, what: &str, actual: String, expected: &str) -> &Self {
        if actual != expected {
            self.lib.fail(
                &format!(
                    "{} {what}: expected {expected:?}, found {actual:?}",
                    self.name
                ),
                Some(self.value),
            );
        }
        self
    }
}

/// Description text with surrounding whitespace removed; missing is empty.
fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

/// Drops source locations, which only add noise to failure messages.
fn strip(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| *k != "loc")
                .map(|(k, v)| (k.clone(), strip(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
        other => other.clone(),
    }
}

/// Removes the common indent, so tests can indent their C++.
fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().skip_while(|l| l.trim().is_empty()).collect();
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}
