use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CodeError, DocumentSnapshot, Form, Node, NodeKind, Path, Segment};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pattern {
    Any,
    Exact(Form),
    Capture { name: String, pattern: Box<Pattern> },
    Kind(NodeKind),
    Sequence(Vec<Pattern>),
    Mapping(Vec<(String, Pattern)>),
}

impl Pattern {
    pub fn capture(name: impl Into<String>, pattern: Pattern) -> Self {
        Self::Capture {
            name: name.into(),
            pattern: Box::new(pattern),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    root: Path,
    include_root: bool,
    mapping_key: Option<String>,
    kind: Option<NodeKind>,
    pattern: Option<Pattern>,
    limit: Option<usize>,
}

impl Query {
    pub fn descendants(root: Path) -> Self {
        Self {
            root,
            include_root: false,
            mapping_key: None,
            kind: None,
            pattern: None,
            limit: None,
        }
    }

    pub fn subtree(root: Path) -> Self {
        Self {
            include_root: true,
            ..Self::descendants(root)
        }
    }

    pub fn with_mapping_key(mut self, key: impl Into<String>) -> Self {
        self.mapping_key = Some(key.into());
        self
    }

    pub const fn with_kind(mut self, kind: NodeKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_pattern(mut self, pattern: Pattern) -> Self {
        self.pattern = Some(pattern);
        self
    }

    pub const fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn execute(&self, document: &DocumentSnapshot) -> Result<Vec<QueryMatch>, CodeError> {
        let root = document.at(&self.root)?;
        let mut candidates = Vec::new();
        if self.include_root {
            candidates.push(root.clone());
        }
        collect_descendants(&root, &mut candidates)?;
        let mut matches = Vec::new();
        for node in candidates {
            if self
                .mapping_key
                .as_ref()
                .is_some_and(|key| node.path().last() != Some(&Segment::Key(key.clone())))
            {
                continue;
            }
            if self.kind.is_some_and(|kind| node.kind() != kind) {
                continue;
            }
            let mut captures = BTreeMap::new();
            if let Some(pattern) = &self.pattern {
                if !pattern_matches(pattern, node.form(), &mut captures) {
                    continue;
                }
            }
            matches.push(QueryMatch { node, captures });
            if self.limit.is_some_and(|limit| matches.len() >= limit) {
                break;
            }
        }
        Ok(matches)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryMatch {
    pub node: Node,
    pub captures: BTreeMap<String, Form>,
}

fn collect_descendants(node: &Node, output: &mut Vec<Node>) -> Result<(), CodeError> {
    for child in node.children()? {
        output.push(child.clone());
        collect_descendants(&child, output)?;
    }
    Ok(())
}

fn pattern_matches(pattern: &Pattern, form: &Form, captures: &mut BTreeMap<String, Form>) -> bool {
    match pattern {
        Pattern::Any => true,
        Pattern::Exact(expected) => expected == form,
        Pattern::Capture { name, pattern } => {
            if !pattern_matches(pattern, form, captures) {
                return false;
            }
            captures.insert(name.clone(), form.clone());
            true
        }
        Pattern::Kind(kind) => form_kind(form) == *kind,
        Pattern::Sequence(patterns) => {
            let Form::Sequence(values) = form else {
                return false;
            };
            patterns.len() == values.len()
                && patterns
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value, captures))
        }
        Pattern::Mapping(patterns) => {
            let Form::Mapping(entries) = form else {
                return false;
            };
            patterns.iter().all(|(key, pattern)| {
                entries
                    .iter()
                    .find(|entry| entry.key == *key)
                    .is_some_and(|entry| pattern_matches(pattern, &entry.value, captures))
            })
        }
    }
}

fn form_kind(form: &Form) -> NodeKind {
    match form {
        Form::Sequence(_) => NodeKind::Sequence,
        Form::Mapping(_) => NodeKind::Mapping,
        _ => NodeKind::Scalar,
    }
}
