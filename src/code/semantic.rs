use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path as FsPath, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ChangeSet, CodeError, CodeErrorKind, Form, Node, Path, Segment, SourceDatabase};

#[derive(Debug, Clone, Default)]
pub struct SemanticIndex {
    definitions: BTreeMap<PathBuf, BTreeSet<String>>,
    imports: BTreeMap<(PathBuf, String), PathBuf>,
    facts: Vec<SemanticFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticKind {
    Definition,
    Import,
    Reference,
    Call,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticFact {
    pub document: PathBuf,
    pub path: Path,
    pub kind: SemanticKind,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_document: Option<PathBuf>,
}

impl SemanticIndex {
    pub fn build(database: &SourceDatabase) -> Result<Self, CodeError> {
        let mut index = Self::default();
        for path in database.paths() {
            let path = path.to_path_buf();
            let document = database.document(&path)?;
            let root = document.root()?;
            let Form::Mapping(entries) = root.form() else {
                continue;
            };
            let definitions = index.definitions.entry(path.clone()).or_default();
            for entry in entries {
                if entry.key.starts_with('=') {
                    continue;
                }
                definitions.insert(entry.key.clone());
                if let Some(import) = import_path(&entry.value) {
                    let target = resolve_import(&path, import);
                    index
                        .imports
                        .insert((path.clone(), entry.key.clone()), target.clone());
                    index.facts.push(SemanticFact {
                        document: path.clone(),
                        path: Path::new(vec![Segment::key(entry.key.clone())]),
                        kind: SemanticKind::Import,
                        symbol: entry.key.clone(),
                        target_document: Some(target),
                    });
                } else {
                    index.facts.push(SemanticFact {
                        document: path.clone(),
                        path: Path::new(vec![Segment::key(entry.key.clone())]),
                        kind: SemanticKind::Definition,
                        symbol: entry.key.clone(),
                        target_document: Some(path.clone()),
                    });
                }
            }
            let mut nodes = Vec::new();
            collect_nodes(&root, &mut nodes)?;
            for node in nodes {
                let Some(reference) = reference_text(&node) else {
                    continue;
                };
                let symbol = reference.trim_start_matches('$').to_string();
                let kind = if node.path().last().is_some_and(
                    |segment| matches!(segment, Segment::Key(key) if key == &reference),
                ) {
                    SemanticKind::Call
                } else {
                    SemanticKind::Reference
                };
                let target_document = symbol
                    .split_once('.')
                    .and_then(|(alias, _)| {
                        index
                            .imports
                            .get(&(path.clone(), alias.to_string()))
                            .cloned()
                    })
                    .or_else(|| Some(path.clone()));
                index.facts.push(SemanticFact {
                    document: path.clone(),
                    path: node.path().clone(),
                    kind,
                    symbol,
                    target_document,
                });
            }
        }
        Ok(index)
    }

    pub fn query(&self, kind: Option<SemanticKind>, symbol: Option<&str>) -> Vec<&SemanticFact> {
        self.facts
            .iter()
            .filter(|fact| kind.is_none_or(|kind| fact.kind == kind))
            .filter(|fact| symbol.is_none_or(|symbol| fact.symbol == symbol))
            .collect()
    }

    pub fn rename_symbol(
        &self,
        database: &SourceDatabase,
        document: impl AsRef<FsPath>,
        old: &str,
        new: &str,
    ) -> Result<ChangeSet, CodeError> {
        validate_symbol_name(new)?;
        let document = normalize_path(document.as_ref());
        let definitions = self.definitions.get(&document).ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("semantic document `{}` does not exist", document.display()),
            )
        })?;
        if !definitions.contains(old) {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("top-level symbol `{old}` does not exist"),
            ));
        }
        if definitions.contains(new) {
            return Err(CodeError::new(
                CodeErrorKind::EditConflict,
                format!("top-level symbol `{new}` already exists"),
            ));
        }

        let mut changes = ChangeSet::new();
        changes = add_reference_renames(
            changes,
            database,
            &document,
            &format!("${old}"),
            &format!("${new}"),
        )?;
        if let Some(self_alias) = module_self_alias(&document) {
            changes = add_reference_renames(
                changes,
                database,
                &document,
                &format!("${self_alias}.{old}"),
                &format!("${self_alias}.{new}"),
            )?;
        }
        for ((importing_document, alias), target) in &self.imports {
            if target == &document {
                changes = add_reference_renames(
                    changes,
                    database,
                    importing_document,
                    &format!("${alias}.{old}"),
                    &format!("${alias}.{new}"),
                )?;
            }
        }
        let definition = database
            .document(&document)?
            .at(&Path::new(vec![Segment::key(old)]))?;
        Ok(changes.rename_mapping_key(definition.locator(), new))
    }

    pub fn rename_import_alias(
        &self,
        database: &SourceDatabase,
        document: impl AsRef<FsPath>,
        old: &str,
        new: &str,
    ) -> Result<ChangeSet, CodeError> {
        validate_symbol_name(new)?;
        let document = normalize_path(document.as_ref());
        if !self
            .imports
            .contains_key(&(document.clone(), old.to_string()))
        {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("import alias `{old}` does not exist"),
            ));
        }
        if self
            .definitions
            .get(&document)
            .is_some_and(|definitions| definitions.contains(new))
        {
            return Err(CodeError::new(
                CodeErrorKind::EditConflict,
                format!("top-level symbol `{new}` already exists"),
            ));
        }
        let changes = add_reference_renames(
            ChangeSet::new(),
            database,
            &document,
            &format!("${old}."),
            &format!("${new}."),
        )?;
        let definition = database
            .document(&document)?
            .at(&Path::new(vec![Segment::key(old)]))?;
        Ok(changes.rename_mapping_key(definition.locator(), new))
    }
}

fn add_reference_renames(
    mut changes: ChangeSet,
    database: &SourceDatabase,
    document_path: &FsPath,
    old_prefix: &str,
    new_prefix: &str,
) -> Result<ChangeSet, CodeError> {
    let document = database.document(document_path)?;
    let mut nodes = Vec::new();
    collect_nodes(&document.root()?, &mut nodes)?;
    for node in nodes {
        let Some(reference) = reference_text(&node) else {
            continue;
        };
        let Some(renamed) = rename_reference(&reference, old_prefix, new_prefix) else {
            continue;
        };
        match node.path().last() {
            Some(Segment::Key(key)) if key == &reference => {
                changes = changes.rename_mapping_key(node.locator(), renamed);
            }
            _ => {
                changes = changes.replace(node.locator(), Form::String(renamed));
            }
        }
    }
    Ok(changes)
}

fn collect_nodes(node: &Node, output: &mut Vec<Node>) -> Result<(), CodeError> {
    for child in node.children()? {
        if child
            .path()
            .last()
            .is_some_and(|segment| matches!(segment, Segment::Key(key) if key.starts_with('=')))
        {
            continue;
        }
        output.push(child.clone());
        collect_nodes(&child, output)?;
    }
    Ok(())
}

fn reference_text(node: &Node) -> Option<String> {
    if let Some(Segment::Key(key)) = node.path().last() {
        if key.starts_with('$') {
            return Some(key.clone());
        }
    }
    match node.form() {
        Form::String(value) if value.starts_with('$') => Some(value.clone()),
        _ => None,
    }
}

fn rename_reference(reference: &str, old_prefix: &str, new_prefix: &str) -> Option<String> {
    if old_prefix.ends_with('.') {
        return reference
            .strip_prefix(old_prefix)
            .map(|suffix| format!("{new_prefix}{suffix}"));
    }
    if reference == old_prefix {
        return Some(new_prefix.to_string());
    }
    reference
        .strip_prefix(&format!("{old_prefix}."))
        .map(|suffix| format!("{new_prefix}.{suffix}"))
}

fn import_path(form: &Form) -> Option<&str> {
    let Form::Mapping(entries) = form else {
        return None;
    };
    entries.iter().find_map(|entry| {
        (entry.key == "$import")
            .then_some(&entry.value)
            .and_then(|form| match form {
                Form::String(value) => Some(value.as_str()),
                _ => None,
            })
    })
}

fn resolve_import(document: &FsPath, import: &str) -> PathBuf {
    if import.starts_with('@') {
        return PathBuf::from(import);
    }
    let parent = document.parent().unwrap_or_else(|| FsPath::new(""));
    normalize_path(&parent.join(import))
}

fn normalize_path(path: &FsPath) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn validate_symbol_name(name: &str) -> Result<(), CodeError> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && name.split('-').all(|part| {
            !part.is_empty()
                && part.chars().enumerate().all(|(index, character)| {
                    if index == 0 {
                        character.is_ascii_lowercase()
                    } else {
                        character.is_ascii_lowercase() || character.is_ascii_digit()
                    }
                })
        });
    if valid {
        Ok(())
    } else {
        Err(CodeError::new(
            CodeErrorKind::EditConflict,
            format!("symbol `{name}` must be kebab-case"),
        ))
    }
}

fn module_self_alias(path: &FsPath) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    Some(
        file_name
            .strip_suffix(".vibra.yaml")
            .or_else(|| file_name.strip_suffix(".vibra"))
            .unwrap_or(file_name)
            .split('.')
            .next()
            .unwrap_or(file_name)
            .to_string(),
    )
}
