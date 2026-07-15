use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::str::FromStr;

use yaml_edit::{Document, YamlNode};

use super::source::{content_hash, node_at_path};
use super::{CodeError, CodeErrorKind, Form, NodeLocator, Path, Segment, SourceDatabase};

#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    Replace {
        target: NodeLocator,
        value: Form,
    },
    Delete {
        target: NodeLocator,
    },
    UpsertMapping {
        target: NodeLocator,
        key: String,
        value: Form,
    },
    InsertMapping {
        target: NodeLocator,
        key: String,
        value: Form,
    },
    RenameMappingKey {
        target: NodeLocator,
        new_key: String,
    },
    InsertSequence {
        target: NodeLocator,
        index: usize,
        value: Form,
    },
    SpliceSequence {
        target: NodeLocator,
        start: usize,
        delete_count: usize,
        values: Vec<Form>,
    },
    Copy {
        source: NodeLocator,
        target: NodeLocator,
        destination: Segment,
    },
    Move {
        source: NodeLocator,
        target: NodeLocator,
        destination: Segment,
    },
    #[doc(hidden)]
    InsertMappingSource {
        target: NodeLocator,
        key: String,
        source: String,
        block_style: bool,
    },
    #[doc(hidden)]
    InsertSequenceSource {
        target: NodeLocator,
        index: usize,
        source: String,
        block_style: bool,
    },
}

impl Edit {
    fn target(&self) -> &NodeLocator {
        match self {
            Self::Replace { target, .. }
            | Self::Delete { target }
            | Self::UpsertMapping { target, .. }
            | Self::InsertMapping { target, .. }
            | Self::RenameMappingKey { target, .. }
            | Self::InsertSequence { target, .. }
            | Self::SpliceSequence { target, .. }
            | Self::Copy { target, .. }
            | Self::Move { target, .. }
            | Self::InsertMappingSource { target, .. }
            | Self::InsertSequenceSource { target, .. } => target,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeSet {
    edits: Vec<Edit>,
}

impl ChangeSet {
    pub const fn new() -> Self {
        Self { edits: Vec::new() }
    }

    pub fn replace(mut self, target: NodeLocator, value: Form) -> Self {
        self.edits.push(Edit::Replace { target, value });
        self
    }

    pub fn delete(mut self, target: NodeLocator) -> Self {
        self.edits.push(Edit::Delete { target });
        self
    }

    pub fn upsert_mapping(
        mut self,
        target: NodeLocator,
        key: impl Into<String>,
        value: Form,
    ) -> Self {
        self.edits.push(Edit::UpsertMapping {
            target,
            key: key.into(),
            value,
        });
        self
    }

    pub fn insert_mapping(
        mut self,
        target: NodeLocator,
        key: impl Into<String>,
        value: Form,
    ) -> Self {
        self.edits.push(Edit::InsertMapping {
            target,
            key: key.into(),
            value,
        });
        self
    }

    pub fn rename_mapping_key(mut self, target: NodeLocator, new_key: impl Into<String>) -> Self {
        self.edits.push(Edit::RenameMappingKey {
            target,
            new_key: new_key.into(),
        });
        self
    }

    pub fn insert_sequence(mut self, target: NodeLocator, index: usize, value: Form) -> Self {
        self.edits.push(Edit::InsertSequence {
            target,
            index,
            value,
        });
        self
    }

    pub fn splice_sequence(
        mut self,
        target: NodeLocator,
        start: usize,
        delete_count: usize,
        values: Vec<Form>,
    ) -> Self {
        self.edits.push(Edit::SpliceSequence {
            target,
            start,
            delete_count,
            values,
        });
        self
    }

    pub fn copy(mut self, source: NodeLocator, target: NodeLocator, destination: Segment) -> Self {
        self.edits.push(Edit::Copy {
            source,
            target,
            destination,
        });
        self
    }

    pub fn move_node(
        mut self,
        source: NodeLocator,
        target: NodeLocator,
        destination: Segment,
    ) -> Self {
        self.edits.push(Edit::Move {
            source,
            target,
            destination,
        });
        self
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChangeSet {
    pub database: SourceDatabase,
    pub changed_files: Vec<PathBuf>,
}

impl SourceDatabase {
    pub fn apply(&self, changes: &ChangeSet) -> Result<AppliedChangeSet, CodeError> {
        validate_preconditions(self, &changes.edits)?;
        let edits = materialize_edits(self, &changes.edits)?;
        validate_non_overlapping(&edits)?;

        let mut database = self.clone();
        let mut by_document: BTreeMap<PathBuf, Vec<&Edit>> = BTreeMap::new();
        for planned in &edits {
            let edit = &planned.edit;
            by_document
                .entry(edit.target().document.clone())
                .or_default()
                .push(edit);
        }

        let mut changed_files = BTreeSet::new();
        for (document_path, mut edits) in by_document {
            let source = database
                .source_document(&document_path)
                .expect("preconditions verify document existence");
            let original = parse_document(source)?;
            edits.sort_by_key(|edit| Reverse(edit_offset(&original, edit)));
            let mut rendered = source.to_string();
            for edit in edits {
                rendered = apply_edit(&rendered, edit)?;
            }
            database.replace_source(&document_path, rendered)?;
            changed_files.insert(document_path);
        }

        Ok(AppliedChangeSet {
            database,
            changed_files: changed_files.into_iter().collect(),
        })
    }
}

struct PlannedEdit {
    edit: Edit,
    move_group: Option<usize>,
}

fn edit_offset(document: &Document, edit: &Edit) -> usize {
    node_at_path(document, &edit.target().path)
        .ok()
        .and_then(|node| node_range(&node).ok())
        .map_or(0, |range| range.0)
}

fn validate_non_overlapping(edits: &[PlannedEdit]) -> Result<(), CodeError> {
    for (index, left) in edits.iter().enumerate() {
        for (right_index, right) in edits.iter().enumerate().skip(index + 1) {
            let left_target = left.edit.target();
            let right_target = right.edit.target();
            if left_target.document == right_target.document
                && (left_target.path.is_prefix_of(&right_target.path)
                    || right_target.path.is_prefix_of(&left_target.path))
                && !same_move_group(left, right)
                && !rename_overlap_is_safe(edits, index, right_index)
            {
                return Err(CodeError::new(
                    CodeErrorKind::OverlappingEdits,
                    format!(
                        "edits at {:?} and {:?} overlap",
                        left_target.path.to_compact_strings(),
                        right_target.path.to_compact_strings()
                    ),
                )
                .at_document(left_target.document.clone()));
            }
        }
    }
    Ok(())
}

fn same_move_group(left: &PlannedEdit, right: &PlannedEdit) -> bool {
    left.move_group.is_some() && left.move_group == right.move_group
}

fn rename_overlap_is_safe(edits: &[PlannedEdit], left_index: usize, right_index: usize) -> bool {
    let left_is_rename = matches!(edits[left_index].edit, Edit::RenameMappingKey { .. });
    let right_is_rename = matches!(edits[right_index].edit, Edit::RenameMappingKey { .. });
    left_is_rename || right_is_rename
}

fn validate_preconditions(database: &SourceDatabase, edits: &[Edit]) -> Result<(), CodeError> {
    for edit in edits {
        validate_locator(database, edit.target())?;
        if let Edit::Copy { source, .. } | Edit::Move { source, .. } = edit {
            validate_locator(database, source)?;
        }
    }
    Ok(())
}

fn validate_locator(database: &SourceDatabase, target: &NodeLocator) -> Result<(), CodeError> {
    let document = database.document(&target.document)?;
    if document.revision() != target.revision {
        return Err(CodeError::new(
            CodeErrorKind::StaleRevision,
            format!(
                "document `{}` changed since the edit was planned",
                target.document.display()
            ),
        )
        .at_document(target.document.clone()));
    }
    let node = document.at(&target.path)?;
    if node.fingerprint() != target.fingerprint {
        return Err(CodeError::new(
            CodeErrorKind::StaleNode,
            "target node no longer matches its planned fingerprint",
        )
        .at_document(target.document.clone())
        .at_path(target.path.clone()));
    }
    Ok(())
}

fn materialize_edits(
    database: &SourceDatabase,
    edits: &[Edit],
) -> Result<Vec<PlannedEdit>, CodeError> {
    let mut materialized = Vec::new();
    for (edit_index, edit) in edits.iter().enumerate() {
        match edit {
            Edit::Copy {
                source,
                target,
                destination,
            }
            | Edit::Move {
                source,
                target,
                destination,
            } => {
                let source_document =
                    database.source_document(&source.document).ok_or_else(|| {
                        CodeError::new(
                            CodeErrorKind::DocumentNotFound,
                            format!(
                                "source document `{}` does not exist",
                                source.document.display()
                            ),
                        )
                    })?;
                let (source_value, block_style) =
                    extract_source_value(source_document, &source.path)?;
                if matches!(edit, Edit::Move { .. })
                    && source.document == target.document
                    && source.path.is_prefix_of(&target.path)
                {
                    return Err(CodeError::new(
                        CodeErrorKind::EditConflict,
                        "cannot move a node into itself or one of its descendants",
                    )
                    .at_document(source.document.clone())
                    .at_path(source.path.clone()));
                }
                let destination = adjusted_move_destination(edit, source, target, destination);
                materialized.push(PlannedEdit {
                    edit: match destination {
                        Segment::Key(key) => Edit::InsertMappingSource {
                            target: target.clone(),
                            key,
                            source: source_value,
                            block_style,
                        },
                        Segment::Index(index) => Edit::InsertSequenceSource {
                            target: target.clone(),
                            index,
                            source: source_value,
                            block_style,
                        },
                    },
                    move_group: matches!(edit, Edit::Move { .. }).then_some(edit_index),
                });
                if matches!(edit, Edit::Move { .. }) {
                    materialized.push(PlannedEdit {
                        edit: Edit::Delete {
                            target: source.clone(),
                        },
                        move_group: Some(edit_index),
                    });
                }
            }
            other => materialized.push(PlannedEdit {
                edit: other.clone(),
                move_group: None,
            }),
        }
    }
    Ok(materialized)
}

fn adjusted_move_destination(
    edit: &Edit,
    source: &NodeLocator,
    target: &NodeLocator,
    destination: &Segment,
) -> Segment {
    let Segment::Index(destination_index) = destination else {
        return destination.clone();
    };
    if !matches!(edit, Edit::Move { .. })
        || source.document != target.document
        || source.path.parent().as_ref() != Some(&target.path)
    {
        return destination.clone();
    }
    match source.path.last() {
        Some(Segment::Index(source_index)) if source_index < destination_index => {
            Segment::index(destination_index - 1)
        }
        _ => destination.clone(),
    }
}

fn apply_edit(source: &str, edit: &Edit) -> Result<String, CodeError> {
    match edit {
        Edit::Replace { target, value } => replace(source, &target.path, value),
        Edit::Delete { target } => delete(source, &target.path),
        Edit::UpsertMapping { target, key, value } => {
            upsert_mapping(source, &target.path, key, value)
        }
        Edit::InsertMapping { target, key, value } => {
            insert_mapping(source, &target.path, key, value)
        }
        Edit::RenameMappingKey { target, new_key } => {
            rename_mapping_key(source, &target.path, new_key)
        }
        Edit::InsertSequence {
            target,
            index,
            value,
        } => insert_sequence(source, &target.path, *index, value),
        Edit::SpliceSequence {
            target,
            start,
            delete_count,
            values,
        } => splice_sequence(source, &target.path, *start, *delete_count, values),
        Edit::InsertMappingSource {
            target,
            key,
            source: value_source,
            block_style,
        } => insert_mapping_source(source, &target.path, key, value_source, *block_style),
        Edit::InsertSequenceSource {
            target,
            index,
            source: value_source,
            block_style,
        } => insert_sequence_source(source, &target.path, *index, value_source, *block_style),
        Edit::Copy { .. } | Edit::Move { .. } => unreachable!("high-level edit was materialized"),
    }
}

fn extract_source_value(source: &str, path: &Path) -> Result<(String, bool), CodeError> {
    let document = parse_document(source)?;
    let node = node_at_path(&document, path)?;
    let (start, end) = node_range(&node)?;
    let block_end = owned_block_range(source, start).end.max(end);
    let block_style = path.parent().is_some_and(|parent_path| {
        node_at_path(&document, &parent_path)
            .ok()
            .is_some_and(|parent| match (parent, path.last()) {
                (YamlNode::Mapping(mapping), Some(Segment::Key(key))) => mapping
                    .entries()
                    .find(|entry| entry.key_matches(key))
                    .and_then(|entry| entry.key_node())
                    .and_then(|key| node_range(&key).ok())
                    .is_some_and(|(key_start, _)| {
                        line_start(source, key_start) != line_start(source, start)
                    }),
                (YamlNode::Sequence(_), Some(Segment::Index(_))) => {
                    !source[line_start(source, start)..start].contains('-')
                }
                _ => false,
            })
    });
    let base_indent = line_indent(source, start);
    let mut value = String::new();
    for (line_index, line) in source[start..block_end].split_inclusive('\n').enumerate() {
        if line_index == 0 {
            value.push_str(line);
        } else {
            value.push_str(line.strip_prefix(&base_indent).unwrap_or(line));
        }
    }
    Ok((value, block_style))
}

fn replace(source: &str, path: &Path, value: &Form) -> Result<String, CodeError> {
    let replacement = value.render()?;
    if path.segments().is_empty() {
        return Ok(ensure_trailing_newline(replacement, source));
    }
    let document = Document::from_str(source).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidSource,
            format!("parse source for edit: {error}"),
        )
    })?;
    let replacement_document = Document::from_str(&replacement).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("parse replacement form: {error}"),
        )
    })?;
    let parent_path = path.parent().expect("non-root path has a parent");
    let parent = node_at_path(&document, &parent_path)?;
    match (parent, path.last().expect("non-root path has a leaf")) {
        (YamlNode::Mapping(mapping), Segment::Key(key)) => {
            if mapping.get(key).is_none() {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("mapping key `{key}` does not exist"),
                ));
            }
            mapping.set(key, replacement_document);
        }
        (YamlNode::Sequence(sequence), Segment::Index(index)) => {
            if !sequence.set(*index, replacement_document) {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("sequence index `{index}` is out of bounds"),
                ));
            }
        }
        (YamlNode::Mapping(_), Segment::Index(index)) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("cannot replace index `{index}` on a mapping"),
            ));
        }
        (YamlNode::Sequence(_), Segment::Key(key)) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("cannot replace key `{key}` on a sequence"),
            ));
        }
        (YamlNode::Scalar(_), _) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                "cannot replace a child of a scalar",
            ));
        }
        (YamlNode::Alias(_) | YamlNode::TaggedNode(_), _) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidSource,
                "aliases and tagged nodes are outside the Vibra source subset",
            ));
        }
    }
    let rendered = document.to_string();
    debug_assert_ne!(content_hash(source), content_hash(&rendered));
    Ok(rendered)
}

fn ensure_trailing_newline(mut replacement: String, original: &str) -> String {
    if original.ends_with('\n') && !replacement.ends_with('\n') {
        replacement.push('\n');
    }
    replacement
}

fn delete(source: &str, path: &Path) -> Result<String, CodeError> {
    let (document, parent, leaf) = editable_parent(source, path)?;
    let range = match (parent, leaf) {
        (YamlNode::Mapping(mapping), Segment::Key(key)) => {
            let entry = mapping
                .entries()
                .find(|entry| entry.key_matches(&key))
                .ok_or_else(|| {
                    CodeError::new(
                        CodeErrorKind::InvalidPath,
                        format!("mapping key `{key}` does not exist"),
                    )
                })?;
            let key = entry.key_node().ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidSource,
                    "mapping entry has no key node",
                )
            })?;
            owned_block_range(source, node_range(&key)?.0)
        }
        (YamlNode::Sequence(sequence), Segment::Index(index)) => {
            let value = sequence.get(index).ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("sequence index `{index}` is out of bounds"),
                )
            })?;
            owned_block_range(source, node_range(&value)?.0)
        }
        (YamlNode::Mapping(_), Segment::Index(index)) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("cannot delete index `{index}` from a mapping"),
            ));
        }
        (YamlNode::Sequence(_), Segment::Key(key)) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("cannot delete key `{key}` from a sequence"),
            ));
        }
        (YamlNode::Scalar(_), _) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidPath,
                "cannot delete a child of a scalar",
            ));
        }
        (YamlNode::Alias(_) | YamlNode::TaggedNode(_), _) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidSource,
                "aliases and tagged nodes are outside the Vibra source subset",
            ));
        }
    };
    let _ = document;
    Ok(replace_byte_range(source, range, ""))
}

fn upsert_mapping(source: &str, path: &Path, key: &str, value: &Form) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Mapping(mapping) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "mapping upsert target is not a mapping",
        ));
    };
    if mapping.get(key).is_some() {
        return replace(source, &path.child(Segment::key(key)), value);
    }
    let insertion = mapping_insertion(&mapping, key, value, source)?;
    Ok(replace_byte_range(
        source,
        insertion.offset..insertion.offset,
        &insertion.text,
    ))
}

fn insert_mapping(source: &str, path: &Path, key: &str, value: &Form) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Mapping(mapping) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "mapping insert target is not a mapping",
        ));
    };
    if mapping.get(key).is_some() {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            format!("mapping key `{key}` already exists"),
        ));
    }
    let insertion = mapping_insertion(&mapping, key, value, source)?;
    Ok(replace_byte_range(
        source,
        insertion.offset..insertion.offset,
        &insertion.text,
    ))
}

fn insert_mapping_source(
    source: &str,
    path: &Path,
    key: &str,
    value_source: &str,
    block_style: bool,
) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Mapping(mapping) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "mapping insert target is not a mapping",
        ));
    };
    if mapping.get(key).is_some() {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            format!("mapping key `{key}` already exists"),
        ));
    }
    let (offset, indent) = mapping_insertion_location(&mapping, source)?;
    let rendered_key = Form::String(key.to_string()).render()?;
    let text = if block_style {
        let mut entry = format!("{rendered_key}:\n");
        entry.push_str(&indent_lines(value_source, "  "));
        indent_lines(&entry, &indent)
    } else {
        indent_lines(
            &format!("{rendered_key}: {}\n", value_source.trim_end_matches('\n')),
            &indent,
        )
    };
    Ok(replace_byte_range(source, offset..offset, &text))
}

fn rename_mapping_key(source: &str, path: &Path, new_key: &str) -> Result<String, CodeError> {
    let (document, parent, leaf) = editable_parent(source, path)?;
    let (YamlNode::Mapping(mapping), Segment::Key(old_key)) = (parent, leaf) else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "mapping rename target must be a mapping value addressed by key",
        ));
    };
    if mapping.get(new_key).is_some() {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            format!("mapping key `{new_key}` already exists"),
        ));
    }
    let entry = mapping
        .entries()
        .find(|entry| entry.key_matches(&old_key))
        .ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::InvalidPath,
                format!("mapping key `{old_key}` does not exist"),
            )
        })?;
    let key_node = entry.key_node().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidSource,
            "mapping entry has no key node",
        )
    })?;
    let range = node_range(&key_node)?;
    let _ = document;
    Ok(replace_byte_range(
        source,
        range.0..range.1,
        &Form::String(new_key.to_string()).render()?,
    ))
}

fn insert_sequence(
    source: &str,
    path: &Path,
    index: usize,
    value: &Form,
) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Sequence(sequence) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "sequence insert target is not a sequence",
        ));
    };
    if index > sequence.len() {
        return Err(CodeError::new(
            CodeErrorKind::InvalidPath,
            format!("sequence insertion index `{index}` is out of bounds"),
        ));
    }
    let insertion = sequence_insertion(&sequence, index, value, source)?;
    Ok(replace_byte_range(
        source,
        insertion.offset..insertion.offset,
        &insertion.text,
    ))
}

fn insert_sequence_source(
    source: &str,
    path: &Path,
    index: usize,
    value_source: &str,
    block_style: bool,
) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Sequence(sequence) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "sequence insert target is not a sequence",
        ));
    };
    if index > sequence.len() {
        return Err(CodeError::new(
            CodeErrorKind::InvalidPath,
            format!("sequence insertion index `{index}` is out of bounds"),
        ));
    }
    let values: Vec<_> = sequence.values().collect();
    let offset = if index < values.len() {
        owned_block_range(source, node_range(&values[index])?.0).start
    } else if let Some(last) = values.last() {
        owned_block_range(source, node_range(last)?.0).end
    } else {
        node_range(&YamlNode::Sequence(sequence.clone()))?.0
    };
    let indent = sequence_indent(&sequence, source)?;
    let text = if block_style {
        let mut item = "-\n".to_string();
        item.push_str(&indent_lines(value_source, "  "));
        indent_lines(&item, &indent)
    } else {
        indent_lines(
            &format!("- {}\n", value_source.trim_end_matches('\n')),
            &indent,
        )
    };
    Ok(replace_byte_range(source, offset..offset, &text))
}

fn splice_sequence(
    source: &str,
    path: &Path,
    start: usize,
    delete_count: usize,
    values: &[Form],
) -> Result<String, CodeError> {
    let document = parse_document(source)?;
    let target = node_at_path(&document, path)?;
    let YamlNode::Sequence(sequence) = target else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "sequence splice target is not a sequence",
        ));
    };
    if start > sequence.len() || start.saturating_add(delete_count) > sequence.len() {
        return Err(CodeError::new(
            CodeErrorKind::InvalidPath,
            "sequence splice range is out of bounds",
        ));
    }
    let existing: Vec<_> = sequence.values().collect();
    let insertion_offset = if start < existing.len() {
        owned_block_range(source, node_range(&existing[start])?.0).start
    } else if let Some(last) = existing.last() {
        owned_block_range(source, node_range(last)?.0).end
    } else {
        node_range(&YamlNode::Sequence(sequence.clone()))?.0
    };
    let removal_end = if delete_count == 0 {
        insertion_offset
    } else {
        owned_block_range(source, node_range(&existing[start + delete_count - 1])?.0).end
    };
    let indent = sequence_indent(&sequence, source)?;
    let replacement = values
        .iter()
        .map(|value| render_sequence_item(value, &indent))
        .collect::<Result<String, _>>()?;
    Ok(replace_byte_range(
        source,
        insertion_offset..removal_end,
        &replacement,
    ))
}

fn editable_parent(source: &str, path: &Path) -> Result<(Document, YamlNode, Segment), CodeError> {
    let Some(parent_path) = path.parent() else {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "document root cannot be deleted or renamed",
        ));
    };
    let leaf = path.last().expect("non-root path has a leaf").clone();
    let document = parse_document(source)?;
    let parent = node_at_path(&document, &parent_path)?;
    Ok((document, parent, leaf))
}

fn parse_document(source: &str) -> Result<Document, CodeError> {
    Document::from_str(source).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidSource,
            format!("parse source for edit: {error}"),
        )
    })
}

struct Insertion {
    offset: usize,
    text: String,
}

fn mapping_insertion(
    mapping: &yaml_edit::Mapping,
    key: &str,
    value: &Form,
    source: &str,
) -> Result<Insertion, CodeError> {
    let (offset, indent) = mapping_insertion_location(mapping, source)?;
    let entry = Form::Mapping(vec![super::Entry {
        key: key.to_string(),
        value: value.clone(),
    }])
    .render()?;
    Ok(Insertion {
        offset,
        text: indent_lines(&entry, &indent),
    })
}

fn mapping_insertion_location(
    mapping: &yaml_edit::Mapping,
    source: &str,
) -> Result<(usize, String), CodeError> {
    let entries: Vec<_> = mapping.entries().collect();
    if let Some(last) = entries.last() {
        let last_key = last.key_node().ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::InvalidSource,
                "mapping entry has no key node",
            )
        })?;
        let offset = owned_block_range(source, node_range(&last_key)?.0).end;
        let key = entries[0].key_node().ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::InvalidSource,
                "mapping entry has no key node",
            )
        })?;
        Ok((offset, line_indent(source, node_range(&key)?.0)))
    } else {
        let (start, _) = node_range(&YamlNode::Mapping(mapping.clone()))?;
        Ok((start, line_indent(source, start)))
    }
}

fn sequence_insertion(
    sequence: &yaml_edit::Sequence,
    index: usize,
    value: &Form,
    source: &str,
) -> Result<Insertion, CodeError> {
    let values: Vec<_> = sequence.values().collect();
    let offset = if index < values.len() {
        owned_block_range(source, node_range(&values[index])?.0).start
    } else if let Some(last) = values.last() {
        owned_block_range(source, node_range(last)?.0).end
    } else {
        node_range(&YamlNode::Sequence(sequence.clone()))?.0
    };
    Ok(Insertion {
        offset,
        text: render_sequence_item(value, &sequence_indent(sequence, source)?)?,
    })
}

fn sequence_indent(sequence: &yaml_edit::Sequence, source: &str) -> Result<String, CodeError> {
    if let Some(first) = sequence.get(0) {
        return Ok(line_indent(source, node_range(&first)?.0));
    }
    let (start, _) = node_range(&YamlNode::Sequence(sequence.clone()))?;
    Ok(line_indent(source, start))
}

fn render_sequence_item(value: &Form, indent: &str) -> Result<String, CodeError> {
    let rendered = value.render()?;
    let mut lines = rendered.lines();
    let first = lines.next().unwrap_or_default();
    let mut output = format!("{indent}- {first}\n");
    for line in lines {
        output.push_str(indent);
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
    Ok(output)
}

fn indent_lines(value: &str, indent: &str) -> String {
    let mut output = String::new();
    for line in value.lines() {
        output.push_str(indent);
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn node_range(node: &YamlNode) -> Result<(usize, usize), CodeError> {
    let range = match node {
        YamlNode::Scalar(node) => node.byte_range(),
        YamlNode::Mapping(node) => node.byte_range(),
        YamlNode::Sequence(node) => node.byte_range(),
        YamlNode::Alias(_) | YamlNode::TaggedNode(_) => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidSource,
                "aliases and tagged nodes are outside the Vibra source subset",
            ));
        }
    };
    Ok((range.start as usize, range.end as usize))
}

fn owned_block_range(source: &str, node_start: usize) -> std::ops::Range<usize> {
    let start = line_start(source, node_start);
    let base_indent = line_indent(source, start).len();
    let mut cursor = line_end(source, start);
    while cursor < source.len() {
        let next_end = line_end(source, cursor);
        let line = &source[cursor..next_end];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            cursor = next_end;
            continue;
        }
        let indent = line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        if indent <= base_indent {
            break;
        }
        cursor = next_end;
    }
    start..cursor
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn line_end(source: &str, offset: usize) -> usize {
    source[offset.min(source.len())..]
        .find('\n')
        .map_or(source.len(), |index| offset + index + 1)
}

fn line_indent(source: &str, offset: usize) -> String {
    let start = line_start(source, offset);
    source[start..]
        .chars()
        .take_while(|character| *character == ' ')
        .collect()
}

fn replace_byte_range(source: &str, range: std::ops::Range<usize>, replacement: &str) -> String {
    let mut output =
        String::with_capacity(source.len() - (range.end - range.start) + replacement.len());
    output.push_str(&source[..range.start]);
    output.push_str(replacement);
    output.push_str(&source[range.end..]);
    output
}
