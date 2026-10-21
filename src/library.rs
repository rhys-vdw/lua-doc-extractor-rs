//! Places declarations together: fields into the tables they name, and tables declared
//! more than once into one. Order is declaration order.

use std::collections::{HashMap, HashSet};

use crate::decl::{Comment, Decl, Field, TypeKind};
use crate::doc::Source;
use crate::name::Path;

#[derive(Debug, Clone)]
pub enum Item {
    /// `[local ]Name = { fields }`
    Table { path: Path, is_local: bool, comment: Comment, fields: Vec<Field>, source: Source },
    /// `Path.name = value`, outside any table declared here
    Field(Field),
    /// `function Path(params) end`
    Function { path: Path, params: Vec<String>, comment: Comment, source: Source },
    /// A comment with no code
    Comment { comment: Comment, source: Source },
}

pub fn resolve(decls: Vec<Decl>) -> Vec<Item> {
    // Enums and `@table`s take fields that name them, wherever those fields are declared.
    // A class's fields are part of its type instead.
    let field_owners: HashSet<String> = decls
        .iter()
        .filter_map(|d| match d {
            Decl::Table { path, .. } | Decl::Type { kind: TypeKind::Enum, path, .. } => Some(path.to_string()),
            _ => None,
        })
        .collect();

    let mut items: Vec<Item> = Vec::new();
    let mut tables: HashMap<String, usize> = HashMap::new();
    let mut table_fields: HashMap<String, Vec<Field>> = HashMap::new();

    for decl in decls {
        match decl {
            Decl::Function { path, params, comment, source } => {
                items.push(Item::Function { path, params, comment, source })
            }
            Decl::Comment { comment, source } => items.push(Item::Comment { comment, source }),
            Decl::Field(field) => {
                let owner = field.path.parent().map(|p| p.to_string()).filter(|p| field_owners.contains(p));
                match owner {
                    Some(owner) => {
                        let field = Field { path: field.path.last(), ..field };
                        table_fields.entry(owner).or_default().push(field);
                    }
                    None => items.push(Item::Field(field)),
                }
            }
            Decl::Table { path, comment, source } => add_table(&mut items, &mut tables, path, false, comment, source),
            Decl::Type { kind, path, comment, source } => {
                add_table(&mut items, &mut tables, path, kind == TypeKind::Class, comment, source)
            }
        }
    }

    for (key, fields) in table_fields {
        if let Some(Item::Table { fields: table, .. }) = tables.get(&key).map(|&i| &mut items[i]) {
            table.extend(fields);
        }
    }
    items
}

/// Adds a table, or merges it into one already declared with the same name.
fn add_table(
    items: &mut Vec<Item>,
    tables: &mut HashMap<String, usize>,
    path: Path,
    is_local: bool,
    comment: Comment,
    source: Source,
) {
    let key = path.to_string();
    match tables.get(&key) {
        Some(&i) => {
            if let Item::Table { comment: existing, .. } = &mut items[i] {
                existing.merge(comment);
            }
        }
        None => {
            tables.insert(key, items.len());
            let is_local = is_local && path.segments.len() == 1;
            items.push(Item::Table { path, is_local, comment, fields: Vec::new(), source });
        }
    }
}
