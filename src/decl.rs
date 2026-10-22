//! What each doc declares: functions, fields, tables, types, or just a comment.
//!
//! `interpret` looks at one doc at a time and never changes it. Placing declarations
//! together (fields into their tables, merging tables) is `library`'s job.

use crate::doc::{Diagnostic, Doc, Raw, Severity, Source, Tag};
use crate::name::Path;
use crate::scan::Pos;
use crate::text::{Line, join, trimmed};
use crate::types::Typed;

/// The comment written above a declaration: a description, then tag lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comment {
    pub description: Vec<Line>,
    /// Tags as written, plus any free text between them, in order.
    pub tags: Vec<Line>,
}

impl Comment {
    /// Adds `other` below this comment, as v3 does when it merges two docs.
    pub fn merge(&mut self, other: Comment) {
        join(&mut self.description, other.description);
        self.tags.extend(other.tags);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    /// `@class`: a local table, whose fields are part of the type.
    Class,
    /// `@enum`: a global table that fields can be added to.
    Enum,
}

/// Each declaration keeps `line`, the C++ line of the tag that declares it, for the code
/// that's generated from it.
#[derive(Debug, Clone)]
pub enum Decl {
    /// `@function Spring.X`
    Function {
        path: Path,
        params: Vec<String>,
        comment: Comment,
        source: Source,
        line: Option<usize>,
    },
    /// `@field Table.name` outside a class, or `@global name`
    Field(Field),
    /// `@table Name`
    Table {
        path: Path,
        comment: Comment,
        source: Source,
        line: Option<usize>,
    },
    /// `@class Name` or `@enum Name`
    Type {
        kind: TypeKind,
        path: Path,
        comment: Comment,
        source: Source,
        line: Option<usize>,
    },
    /// A doc that declares nothing, e.g. an `@alias`
    Comment { comment: Comment, source: Source },
}

#[derive(Debug, Clone)]
pub struct Field {
    pub path: Path,
    pub typed: Typed,
    pub comment: Comment,
    pub source: Source,
    pub line: Option<usize>,
}

pub fn interpret(doc: Doc, diagnostics: &mut Vec<Diagnostic>) -> Vec<Decl> {
    let Doc {
        source,
        description,
        tags,
    } = doc;
    let is_class = tags.iter().any(|t| matches!(t, Tag::Class { .. }));
    let has_tags = tags.iter().any(|t| !matches!(t, Tag::Text(_)));

    let mut functions: Vec<(Path, Vec<Line>, Option<usize>)> = Vec::new();
    let mut table: Option<(Path, Vec<Line>, Option<usize>)> = None;
    let mut enum_: Option<(Path, Vec<Line>)> = None;
    let mut class: Option<(Path, Option<usize>)> = None;
    // Each field, and whether it's a `@global`.
    let mut fields: Vec<(Field, bool)> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    // Tags (and free text) in order, for the comment above the declaration.
    let mut lines: Vec<Line> = Vec::new();
    // The same tags without the free text, for fields that take the doc's tags.
    let mut tag_lines: Vec<Line> = Vec::new();
    // Free text that no field claimed.
    let mut free_text: Vec<Line> = Vec::new();
    let mut pending_text: Option<Vec<Line>> = None;

    for tag in tags {
        // Text directly above a generated field is its description, as emmylua reads a
        // description above `@field`. In a class, emmylua reads it itself.
        if let Some(text) = pending_text.take() {
            match &tag {
                Tag::Field { .. } if !is_class => pending_text = Some(text),
                _ => {
                    lines.extend(text.iter().cloned());
                    free_text.extend(text);
                }
            }
        }
        let before = lines.len();
        match tag {
            Tag::Function {
                path,
                description,
                line,
            } => functions.push((path, description, line)),
            Tag::Table {
                path,
                description,
                line,
            } => table = Some((path, description, line)),
            Tag::Enum { path, raw } => enum_ = Some((path, render(&raw))),
            Tag::Class { path, raw } => {
                class = Some((path, raw.lines[0].src));
                lines.extend(render(&raw));
            }
            Tag::Field { raw, .. } if is_class => lines.extend(render(&raw)),
            Tag::Field {
                path,
                mut typed,
                source: field_source,
                raw,
            } => {
                if let Some(above) = pending_text.take() {
                    let inline = std::mem::take(&mut typed.description);
                    typed.description = trimmed(above);
                    join(&mut typed.description, inline);
                }
                let field = Field {
                    path,
                    typed,
                    comment: Comment::default(),
                    source: field_source,
                    line: raw.lines[0].src,
                };
                fields.push((field, raw.name == "global"));
            }
            Tag::Param { name, raw } => {
                params.push(name);
                lines.extend(render(&raw));
            }
            Tag::Other(raw) => lines.extend(render(&raw)),
            // Used to route the doc, never written out.
            Tag::Env { .. } => {}
            Tag::Text(text) => pending_text = Some(text),
        }
        tag_lines.extend(lines[before..].iter().cloned());
    }
    if let Some(text) = pending_text {
        lines.extend(text.iter().cloned());
        free_text.extend(text);
    }

    let comment = |extra: Vec<Line>| {
        let mut description = description.clone();
        join(&mut description, extra);
        Comment {
            description,
            tags: lines.clone(),
        }
    };
    let mut decls = Vec::new();

    for (path, inline, line) in functions {
        decls.push(Decl::Function {
            path,
            params: params.clone(),
            comment: comment(inline),
            source: source.clone(),
            line,
        });
    }

    // Fields named without their table (`@field STOP 0`) belong to the doc's enum or table.
    let owner = enum_
        .as_ref()
        .map(|(p, _)| p)
        .or(table.as_ref().map(|(p, _, _)| p))
        .cloned();
    if let Some((path, enum_line)) = enum_ {
        // `@enum` goes first, so emmylua attaches the tags that follow it to the enum.
        let line = enum_line[0].src;
        let mut c = comment(vec![]);
        c.tags = [enum_line, c.tags].concat();
        decls.push(Decl::Type {
            kind: TypeKind::Enum,
            path,
            comment: c,
            source: source.clone(),
            line,
        });
    }
    if let Some((path, inline, line)) = table {
        decls.push(Decl::Table {
            path,
            comment: comment(inline),
            source: source.clone(),
            line,
        });
    }
    // With an explicit `@table`, the class is just a tag on it.
    if let Some((path, line)) =
        class.filter(|_| decls.iter().all(|d| !matches!(d, Decl::Table { .. })))
    {
        decls.push(Decl::Type {
            kind: TypeKind::Class,
            path,
            comment: comment(vec![]),
            source: source.clone(),
            line,
        });
    }

    // A `@field` needs a table: its own (`@field Table.name`), or the doc's enum or table.
    // A global is declared with `@global`, so there's one way to do it.
    let fields: Vec<Field> = fields
        .into_iter()
        .filter_map(|(field, is_global)| {
            if is_global || owner.is_some() || field.path.segments.len() > 1 {
                return Some(field);
            }
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                path: source.path.clone(),
                pos: field_pos(&source, field.line),
                message: format!(
                    "@field {} has no table; declare a global with `@global {}`",
                    field.path, field.path
                ),
            });
            None
        })
        .collect();

    // A doc that only declares fields gives them its description and tags. Free text
    // that isn't directly above a field stays unattached, as emmylua reads it.
    let fields_only = decls.is_empty() && !fields.is_empty();
    if fields_only && !free_text.is_empty() {
        decls.push(Decl::Comment {
            comment: Comment {
                description: Vec::new(),
                tags: free_text,
            },
            source: source.clone(),
        });
    }
    for mut field in fields {
        if let Some(owner) = &owner
            && field.path.segments.len() == 1
        {
            field.path = Path {
                segments: [owner.segments.clone(), field.path.segments].concat(),
                is_method: false,
            };
        }
        if fields_only {
            // Where this goes relative to the field's own description is decided by `emit`.
            field.comment = Comment {
                description: description.clone(),
                tags: tag_lines.clone(),
            };
        }
        decls.push(Decl::Field(field));
    }

    if decls.is_empty() {
        if !has_tags {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                path: source.path.clone(),
                pos: source.start,
                message: "this doc has only a description, so it isn't attached to anything".into(),
            });
        }
        decls.push(Decl::Comment {
            comment: comment(vec![]),
            source,
        });
    }
    decls
}

/// The position of a tag's line, for a diagnostic: its line, or the doc's start.
fn field_pos(source: &Source, line: Option<usize>) -> Pos {
    match line {
        Some(line) => Pos { line, col: 1 },
        None => source.start,
    }
}

/// A tag as Lua comment lines, without trailing blank lines.
fn render(raw: &Raw) -> Vec<Line> {
    let mut lines = raw.lines.clone();
    while lines.len() > 1 && lines.last().is_some_and(Line::is_blank) {
        lines.pop();
    }
    let first = format!("@{} {}", raw.name, lines[0].text);
    lines[0] = Line::new(first.trim_end(), lines[0].src);
    lines
}
