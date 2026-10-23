//! `@env`: routes each doc to an output file by the environments (Lua states) its subject is
//! registered in, as v3 does.
//!
//! A doc's environments come from its own `@env`, or from the file's marker: a doc that holds
//! only `@env`, first in the file. `--env name[,name...]=file` names the output for each exact
//! set of environments. Docs without an environment go to their usual per-source file, and
//! without any `--env` the tags change nothing.

use std::collections::BTreeMap;

use crate::doc::{Diagnostic, Doc, Severity, Tag};

/// The output file for each set of environments, by `key`.
#[derive(Debug, Clone, Default)]
pub struct Routes(BTreeMap<String, String>);

impl Routes {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The output file for exactly these environments.
    pub fn get(&self, envs: &[String]) -> Option<&str> {
        self.0.get(&key(envs)).map(String::as_str)
    }
}

/// The names of a set of environments, sorted and without duplicates, so `client, server`
/// and `server,client` route the same way.
pub fn key(envs: &[String]) -> String {
    let mut names: Vec<&str> = envs
        .iter()
        .map(|e| e.trim())
        .filter(|e| !e.is_empty())
        .collect();
    names.sort_unstable();
    names.dedup();
    names.join(",")
}

/// Parses `--env name[,name...]=file` values. A malformed or repeated route is an error.
pub fn parse_routes(values: &[String]) -> (Routes, Vec<String>) {
    let mut routes = BTreeMap::new();
    let mut errors = Vec::new();
    for value in values {
        let parsed = value.split_once('=').and_then(|(names, file)| {
            let names: Vec<String> = names.split(',').map(str::to_string).collect();
            let (key, file) = (key(&names), file.trim());
            (!key.is_empty() && !file.is_empty()).then(|| (key, file.to_string()))
        });
        match parsed {
            None => errors.push(format!("--env '{value}': expected name[,name...]=file")),
            Some((key, _)) if routes.contains_key(&key) => {
                errors.push(format!("--env '{value}': '{key}' is already routed"));
            }
            Some((key, file)) => {
                routes.insert(key, file);
            }
        }
    }
    (Routes(routes), errors)
}

/// Removes the file's `@env` marker and gives its environments to every doc that has none of
/// its own. A marker that isn't the file's first doc, or a second one, is an error, and isn't
/// applied.
pub fn apply_file_env(docs: &mut Vec<Doc>, diagnostics: &mut Vec<Diagnostic>) {
    let markers: Vec<usize> = (0..docs.len())
        .filter(|&i| docs[i].is_env_marker())
        .collect();
    let mut report = |doc: &Doc, message: &str| {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            path: doc.source.path.clone(),
            pos: doc.source.start,
            message: message.to_string(),
        })
    };
    // One error per file, as in v3.
    let envs = match markers.as_slice() {
        [] => return,
        [0] => docs[0].envs(),
        [_] => {
            report(
                &docs[markers[0]],
                "A file-level `@env` doc must be the first doc in the file.",
            );
            Vec::new()
        }
        [_, second, ..] => {
            report(
                &docs[*second],
                "More than one file-level `@env` doc. A file can have only one.",
            );
            Vec::new()
        }
    };
    for &i in markers.iter().rev() {
        docs.remove(i);
    }
    if envs.is_empty() {
        return;
    }
    for doc in docs.iter_mut().filter(|d| d.envs().is_empty()) {
        doc.tags.push(Tag::Env {
            names: envs.clone(),
        });
    }
}
