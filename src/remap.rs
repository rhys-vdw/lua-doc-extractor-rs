//! Moves a Lua checker's diagnostics on the generated library back onto the C++ they came from,
//! using the source map, so they can be reported on a pull request's C++ diff.
//!
//! Reads LuaLS's `check.json` (from `lua-language-server --check`) and `emmylua_check -f json`.
//! Both hold LSP diagnostics, with 0-based lines; only the wrapper differs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A checker's diagnostic, on a generated Lua file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub file: PathBuf,
    /// 0-based, as in LSP.
    pub line: usize,
    pub level: Level,
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Notice,
}

/// A diagnostic placed on a source file: the C++ line a generated line came from, or the
/// checked file itself when it isn't generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remapped {
    /// Relative to the working directory, with forward slashes.
    pub path: String,
    /// 1-based.
    pub line: usize,
    pub level: Level,
    pub code: Option<String>,
    pub message: String,
}

/// Reads a checker's JSON output: LuaLS's object of file URI to diagnostics, or
/// `emmylua_check`'s array of `{ file, diagnostics }`. Hints are left out.
pub fn read_check(json: &Value) -> Result<Vec<CheckDiagnostic>, String> {
    let files: Vec<(PathBuf, &Value)> = match json {
        Value::Object(files) => files
            .iter()
            .map(|(uri, diagnostics)| (uri_to_path(uri), diagnostics))
            .collect(),
        Value::Array(files) => files
            .iter()
            .map(|f| match (f["file"].as_str(), f.get("diagnostics")) {
                (Some(file), Some(diagnostics)) => Ok((PathBuf::from(file), diagnostics)),
                _ => Err("expected { file, diagnostics } entries".to_string()),
            })
            .collect::<Result<_, _>>()?,
        _ => return Err("expected LuaLS or emmylua_check JSON".into()),
    };

    let mut out = Vec::new();
    for (file, diagnostics) in files {
        let diagnostics = diagnostics
            .as_array()
            .ok_or_else(|| format!("{}: expected a list of diagnostics", file.display()))?;
        for d in diagnostics {
            let level = match d["severity"].as_u64() {
                Some(1) => Level::Error,
                Some(2) => Level::Warning,
                Some(3) => Level::Notice,
                // Hints, e.g. emmylua's `unused`.
                _ => continue,
            };
            let line = d["range"]["start"]["line"]
                .as_u64()
                .ok_or_else(|| format!("{}: a diagnostic has no range", file.display()))?;
            out.push(CheckDiagnostic {
                file: file.clone(),
                line: line as usize,
                level,
                code: match &d["code"] {
                    Value::String(code) => Some(code.clone()),
                    Value::Number(code) => Some(code.to_string()),
                    _ => None,
                },
                message: d["message"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
    Ok(out)
}

/// A `file://` URI as a path: `file:///c%3A/x/a.lua` is `c:/x/a.lua`, `file:///x/a.lua` is
/// `/x/a.lua`.
pub fn uri_to_path(uri: &str) -> PathBuf {
    let Some(rest) = uri.strip_prefix("file://") else {
        return PathBuf::from(uri);
    };
    let path = percent_decode(rest);
    // A Windows drive: `/c:/x`.
    let bytes = path.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        return PathBuf::from(&path[1..]);
    }
    PathBuf::from(path)
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The source map: for each output (relative to the output directory), its sources and the
/// `(source, line)` of each line.
pub struct SourceMap {
    files: HashMap<String, MapFile>,
}

struct MapFile {
    sources: Vec<String>,
    lines: Vec<Option<(usize, usize)>>,
}

impl SourceMap {
    /// Reads a version 2 source map, as written by `--sourcemap`.
    pub fn from_json(json: &Value) -> Result<SourceMap, String> {
        if json["version"] != 2 {
            return Err(format!(
                "expected source map version 2, found {}",
                json["version"]
            ));
        }
        let files = json["files"]
            .as_object()
            .ok_or("the source map has no files")?;
        let mut map = HashMap::new();
        for (name, file) in files {
            let bad = || format!("source map entry '{name}' is malformed");
            let sources = file["sources"]
                .as_array()
                .ok_or_else(bad)?
                .iter()
                .map(|s| s.as_str().map(str::to_string).ok_or_else(bad))
                .collect::<Result<Vec<_>, _>>()?;
            let lines = file["lines"]
                .as_array()
                .ok_or_else(bad)?
                .iter()
                .map(|l| match l {
                    Value::Null => Ok(None),
                    Value::Array(pair) => match (pair.first(), pair.get(1)) {
                        (Some(s), Some(n)) => match (s.as_u64(), n.as_u64()) {
                            (Some(s), Some(n)) if (s as usize) < sources.len() => {
                                Ok(Some((s as usize, n as usize)))
                            }
                            _ => Err(bad()),
                        },
                        _ => Err(bad()),
                    },
                    _ => Err(bad()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            map.insert(name.clone(), MapFile { sources, lines });
        }
        Ok(SourceMap { files: map })
    }
}

/// Places each diagnostic on its source. `dest` is the output directory the map's paths are
/// relative to, and `cwd` the directory reported paths are relative to.
pub fn remap(
    diagnostics: &[CheckDiagnostic],
    map: &SourceMap,
    dest: &Path,
    cwd: &Path,
) -> Vec<Remapped> {
    let dest = canonical(dest);
    let cwd = canonical(cwd);
    diagnostics
        .iter()
        .map(|d| {
            let file = canonical(&d.file);
            let output = file.strip_prefix(&dest).ok().map(slashes);
            let origin = output.and_then(|name| map.files.get(&name)).and_then(|f| {
                let (source, line) = (*f.lines.get(d.line)?)?;
                Some((f.sources[source].clone(), line))
            });
            let (path, line) = origin.unwrap_or_else(|| {
                // Not generated, or a generated line with no source: report it where it is.
                let path = file.strip_prefix(&cwd).map(slashes);
                (path.unwrap_or_else(|_| slashes(&file)), d.line + 1)
            });
            Remapped {
                path,
                line,
                level: d.level,
                code: d.code.clone(),
                message: d.message.clone(),
            }
        })
        .collect()
}

/// The path with symlinks and `.` resolved, if it exists. A path can be reported by a checker
/// that ran elsewhere, so a missing one is kept as written.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A GitHub Actions workflow command, which shows on the line in a pull request.
pub fn github(d: &Remapped) -> String {
    annotation(
        d.level,
        &d.path,
        d.line,
        None,
        d.code.as_deref(),
        &d.message,
    )
}

/// A GitHub Actions workflow command (`::error file=...,line=...::message`). GitHub reads these
/// from a step's standard output, and shows them on the file's line in a pull request.
pub fn annotation(
    level: Level,
    path: &str,
    line: usize,
    col: Option<usize>,
    title: Option<&str>,
    message: &str,
) -> String {
    let command = match level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Notice => "notice",
    };
    let col = col.map(|c| format!(",col={c}")).unwrap_or_default();
    let title = title
        .map(|t| format!(",title={}", escape_property(t)))
        .unwrap_or_default();
    format!(
        "::{command} file={},line={line}{col}{title}::{}",
        escape_property(path),
        escape_data(message)
    )
}

/// `path:line: level: message [code]`.
pub fn text(d: &Remapped) -> String {
    let level = match d.level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Notice => "note",
    };
    let code = d
        .code
        .as_deref()
        .map(|c| format!(" [{c}]"))
        .unwrap_or_default();
    format!("{}:{}: {level}: {}{code}", d.path, d.line, d.message)
}

fn escape_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(s: &str) -> String {
    escape_data(s).replace(':', "%3A").replace(',', "%2C")
}
