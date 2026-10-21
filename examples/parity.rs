//! Compares what emmylua understands from two generated libraries.
//!
//! cargo run --example parity -- <stubs dir> <old dir> <new dir>
//!
//! Each library is analysed together with the hand-written stubs, and reduced to a set
//! of facts (one line per name, type, param, return, description, ...). Facts found in
//! only one library are printed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};
use serde_json::Value;

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [stubs, old, new] = args.as_slice() else {
        panic!("usage: parity <stubs dir> <old dir> <new dir>");
    };

    // emmylua's type rendering varies between runs (union order, alias expansion), so only
    // facts that come out the same in two runs are compared (workaround A, UPSTREAM.md #6).
    let (old_facts, old_unstable) = stable_facts(stubs, old, "old");
    let (new_facts, new_unstable) = stable_facts(stubs, new, "new");
    for fact in old_unstable.union(&new_unstable) {
        println!("? {fact}");
    }

    let removed: Vec<_> = old_facts.difference(&new_facts).collect();
    let added: Vec<_> = new_facts.difference(&old_facts).collect();
    for fact in &removed {
        println!("- {fact}");
    }
    for fact in &added {
        println!("+ {fact}");
    }
    eprintln!(
        "{} facts in old, {} in new; {} only in old, {} only in new",
        old_facts.len(),
        new_facts.len(),
        removed.len(),
        added.len()
    );
}

/// Facts found in both of two runs, and those found in only one.
fn stable_facts(stubs: &Path, library: &Path, label: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let a = facts(stubs, library, &format!("{label}-a"));
    let b = facts(stubs, library, &format!("{label}-b"));
    let stable = a.intersection(&b).cloned().collect();
    let unstable = a.symmetric_difference(&b).cloned().collect();
    (stable, unstable)
}

fn facts(stubs: &Path, library: &Path, label: &str) -> BTreeSet<String> {
    let work = std::env::temp_dir().join(format!("ldx-parity-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    copy_lua(stubs, &work.join("stubs"));
    copy_lua(library, &work.join("generated"));

    let json = work.join("doc.json");
    run_doc_cli(CmdArgs {
        config: None,
        workspace: vec![work.clone()],
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

    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    let mut out = BTreeSet::new();
    for global in doc["globals"].as_array().unwrap() {
        let name = global["name"].as_str().unwrap();
        match global["type"].as_str().unwrap() {
            "table" => {
                for m in members(global) {
                    member_facts(&mut out, name, m);
                }
            }
            _ => field_facts(&mut out, "global", "", global),
        }
    }
    for ty in doc["types"].as_array().unwrap() {
        let kind = ty["type"].as_str().unwrap();
        let name = ty["name"].as_str().unwrap();
        out.insert(format!("{kind} {name}"));
        if let Some(t) = ty["typ"].as_str() {
            out.insert(format!("{kind} {name}: {t}"));
        }
        for base in ty["bases"].as_array().into_iter().flatten() {
            out.insert(format!("{kind} {name} extends {}", base.as_str().unwrap()));
        }
        property_facts(&mut out, &format!("{kind} {name}"), ty);
        for m in members(ty) {
            member_facts(&mut out, name, m);
        }
    }
    out
}

fn members(v: &Value) -> impl Iterator<Item = &Value> {
    v["members"].as_array().into_iter().flatten()
}

fn member_facts(out: &mut BTreeSet<String>, owner: &str, m: &Value) {
    let name = format!("{owner}.{}", m["name"].as_str().unwrap());
    match m["type"].as_str().unwrap() {
        "fn" => {
            out.insert(format!("fn {name}"));
            for (list, label) in [("params", "param"), ("returns", "return")] {
                for (i, p) in m[list].as_array().into_iter().flatten().enumerate() {
                    let pname = p["name"].as_str().unwrap_or("_");
                    let typ = p["typ"].as_str().unwrap_or("?");
                    out.insert(format!("fn {name} {label} {i}: {pname}: {typ}"));
                    if let Some(desc) = text(&p["desc"]) {
                        out.insert(format!("fn {name} {label} {i} desc: {desc}"));
                    }
                }
            }
            property_facts(out, &format!("fn {name}"), m);
        }
        _ => field_facts(out, "field", owner, m),
    }
}

fn field_facts(out: &mut BTreeSet<String>, kind: &str, owner: &str, f: &Value) {
    let name = f["name"].as_str().unwrap();
    let name = if owner.is_empty() { name.to_string() } else { format!("{owner}.{name}") };
    let typ = f["typ"].as_str().unwrap_or("?");
    match f["literal"].as_str() {
        Some(literal) => out.insert(format!("{kind} {name}: {typ} = {literal}")),
        None => out.insert(format!("{kind} {name}: {typ}")),
    };
    property_facts(out, &format!("{kind} {name}"), f);
}

fn property_facts(out: &mut BTreeSet<String>, subject: &str, v: &Value) {
    if let Some(desc) = text(&v["description"]) {
        out.insert(format!("{subject} desc: {desc}"));
    }
    if v["deprecated"].as_bool() == Some(true) {
        out.insert(format!("{subject} deprecated: {}", v["deprecation_reason"].as_str().unwrap_or("")));
    }
    for tag in v["tag_content"].as_array().into_iter().flatten() {
        let content = normalise(tag["content"].as_str().unwrap_or(""));
        out.insert(format!("{subject} @{} {content}", tag["tag_name"].as_str().unwrap()));
    }
}

/// Non-empty text with v3 source links removed and whitespace collapsed.
fn text(v: &Value) -> Option<String> {
    let s = normalise(v.as_str()?);
    (!s.is_empty()).then_some(s)
}

fn normalise(s: &str) -> String {
    let mut s = s.to_string();
    while let Some(start) = s.find("[<a href=") {
        let end = s[start..].find("</a>]").map_or(s.len(), |e| start + e + 5);
        s.replace_range(start..end, "");
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn copy_lua(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_lua(&path, &target);
        } else if path.extension().is_some_and(|e| e == "lua") {
            std::fs::create_dir_all(to).unwrap();
            std::fs::copy(&path, &target).unwrap();
        }
    }
}
