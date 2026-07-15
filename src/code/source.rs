use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path as FsPath, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use yaml_edit::{Document, YamlNode};

use super::{Form, Path, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeErrorKind {
    DocumentNotFound,
    InvalidSource,
    InvalidForm,
    InvalidPath,
    StaleRevision,
    StaleNode,
    OverlappingEdits,
    EditConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeError {
    pub kind: CodeErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Path>,
}

impl CodeError {
    pub fn new(kind: CodeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            document: None,
            path: None,
        }
    }

    pub fn at_document(mut self, document: impl Into<PathBuf>) -> Self {
        self.document = Some(document.into());
        self
    }

    pub fn at_path(mut self, path: Path) -> Self {
        self.path = Some(path);
        self
    }
}

impl fmt::Display for CodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for CodeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Scalar,
    Mapping,
    Sequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeLocator {
    pub document: PathBuf,
    pub revision: String,
    pub path: Path,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    locator: NodeLocator,
    kind: NodeKind,
    source: String,
    form: Form,
    document_source: String,
}

impl Node {
    pub fn locator(&self) -> NodeLocator {
        self.locator.clone()
    }

    pub fn document(&self) -> &FsPath {
        &self.locator.document
    }

    pub fn revision(&self) -> &str {
        &self.locator.revision
    }

    pub fn path(&self) -> &Path {
        &self.locator.path
    }

    pub fn fingerprint(&self) -> &str {
        &self.locator.fingerprint
    }

    pub const fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn form(&self) -> &Form {
        &self.form
    }

    pub fn parent(&self, document: &DocumentSnapshot) -> Result<Option<Node>, CodeError> {
        self.path()
            .parent()
            .map(|path| document.at(&path))
            .transpose()
    }

    pub fn children(&self) -> Result<Vec<Node>, CodeError> {
        let segments: Vec<Segment> = match &self.form {
            Form::Mapping(entries) => entries
                .iter()
                .map(|entry| Segment::key(entry.key.clone()))
                .collect(),
            Form::Sequence(values) => (0..values.len()).map(Segment::index).collect(),
            _ => Vec::new(),
        };
        let document = DocumentSnapshot {
            path: self.locator.document.clone(),
            source: self.document_source.clone(),
            revision: self.locator.revision.clone(),
        };
        segments
            .into_iter()
            .map(|segment| document.at(&self.locator.path.child(segment)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceDocument {
    path: PathBuf,
    source: String,
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDatabase {
    root: PathBuf,
    documents: BTreeMap<PathBuf, SourceDocument>,
}

impl SourceDatabase {
    pub fn discover(root: impl AsRef<FsPath>) -> Result<Self, CodeError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("resolve workspace {}: {error}", root.as_ref().display()),
            )
        })?;
        let mut sources = Vec::new();
        discover_sources(&root, &root, &mut sources)?;
        Self::from_sources(root, sources)
    }

    pub fn from_sources<I>(root: PathBuf, sources: I) -> Result<Self, CodeError>
    where
        I: IntoIterator<Item = (PathBuf, String)>,
    {
        let mut documents = BTreeMap::new();
        for (path, source) in sources {
            crate::yaml_subset::validate_yaml_subset_or_err(&source, &path).map_err(|error| {
                CodeError::new(CodeErrorKind::InvalidSource, error.to_string())
                    .at_document(path.clone())
            })?;
            Document::from_str(&source).map_err(|error| {
                CodeError::new(
                    CodeErrorKind::InvalidSource,
                    format!("parse {}: {error}", path.display()),
                )
                .at_document(path.clone())
            })?;
            let value: serde_yaml::Value = serde_yaml::from_str(&source).map_err(|error| {
                CodeError::new(
                    CodeErrorKind::InvalidSource,
                    format!("parse annotations in {}: {error}", path.display()),
                )
                .at_document(path.clone())
            })?;
            crate::annotations::validate(&value).map_err(|error| {
                CodeError::new(CodeErrorKind::InvalidSource, error.to_string())
                    .at_document(path.clone())
            })?;
            let path = normalize_path(path);
            let revision = content_hash(&source);
            documents.insert(
                path.clone(),
                SourceDocument {
                    path,
                    source,
                    revision,
                },
            );
        }
        Ok(Self { root, documents })
    }

    pub fn root(&self) -> &FsPath {
        &self.root
    }

    pub fn document(&self, path: impl AsRef<FsPath>) -> Result<DocumentSnapshot, CodeError> {
        let path = normalize_path(path.as_ref().to_path_buf());
        let document = self.documents.get(&path).ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("source document `{}` does not exist", path.display()),
            )
            .at_document(path.clone())
        })?;
        Ok(DocumentSnapshot {
            path: document.path.clone(),
            source: document.source.clone(),
            revision: document.revision.clone(),
        })
    }

    pub fn paths(&self) -> impl Iterator<Item = &FsPath> {
        self.documents.keys().map(PathBuf::as_path)
    }

    pub(crate) fn source_document(&self, path: &FsPath) -> Option<&str> {
        self.documents
            .get(path)
            .map(|document| document.source.as_str())
    }

    pub(crate) fn replace_source(
        &mut self,
        path: &FsPath,
        source: String,
    ) -> Result<(), CodeError> {
        let document = self.documents.get_mut(path).ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("source document `{}` does not exist", path.display()),
            )
            .at_document(path.to_path_buf())
        })?;
        document.revision = content_hash(&source);
        document.source = source;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSnapshot {
    path: PathBuf,
    source: String,
    revision: String,
}

impl DocumentSnapshot {
    pub fn path(&self) -> &FsPath {
        &self.path
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn root(&self) -> Result<Node, CodeError> {
        self.at(&Path::root())
    }

    pub fn at(&self, path: &Path) -> Result<Node, CodeError> {
        let document = Document::from_str(&self.source).map_err(|error| {
            CodeError::new(
                CodeErrorKind::InvalidSource,
                format!("parse {}: {error}", self.path.display()),
            )
            .at_document(self.path.clone())
        })?;
        let node = node_at_path(&document, path)
            .map_err(|error| error.at_document(self.path.clone()).at_path(path.clone()))?;
        let source = node.to_string();
        let form = form_from_yaml_node(&node)?;
        Ok(Node {
            locator: NodeLocator {
                document: self.path.clone(),
                revision: self.revision.clone(),
                path: path.clone(),
                fingerprint: content_hash(&source),
            },
            kind: node_kind(&node)?,
            source,
            form,
            document_source: self.source.clone(),
        })
    }
}

pub(crate) fn node_at_path(document: &Document, path: &Path) -> Result<YamlNode, CodeError> {
    let mut node = root_node(document)?;
    for segment in path.segments() {
        node = match (node, segment) {
            (YamlNode::Mapping(mapping), Segment::Key(key)) => {
                mapping.get(key).ok_or_else(|| {
                    CodeError::new(
                        CodeErrorKind::InvalidPath,
                        format!("mapping key `{key}` does not exist"),
                    )
                })?
            }
            (YamlNode::Sequence(sequence), Segment::Index(index)) => {
                sequence.get(*index).ok_or_else(|| {
                    CodeError::new(
                        CodeErrorKind::InvalidPath,
                        format!("sequence index `{index}` is out of bounds"),
                    )
                })?
            }
            (YamlNode::Mapping(_), Segment::Index(index)) => {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("cannot use index `{index}` on a mapping"),
                ));
            }
            (YamlNode::Sequence(_), Segment::Key(key)) => {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("cannot use key `{key}` on a sequence"),
                ));
            }
            (YamlNode::Scalar(_), _) => {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidPath,
                    "cannot descend through a scalar",
                ));
            }
            (YamlNode::Alias(_) | YamlNode::TaggedNode(_), _) => {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidSource,
                    "aliases and tagged nodes are outside the Vibra source subset",
                ));
            }
        };
    }
    Ok(node)
}

pub(crate) fn root_node(document: &Document) -> Result<YamlNode, CodeError> {
    if let Some(mapping) = document.as_mapping() {
        return Ok(YamlNode::Mapping(mapping));
    }
    if let Some(sequence) = document.as_sequence() {
        return Ok(YamlNode::Sequence(sequence));
    }
    if let Some(scalar) = document.as_scalar() {
        return Ok(YamlNode::Scalar(scalar));
    }
    Err(CodeError::new(
        CodeErrorKind::InvalidSource,
        "source document has no root node",
    ))
}

fn node_kind(node: &YamlNode) -> Result<NodeKind, CodeError> {
    match node {
        YamlNode::Scalar(_) => Ok(NodeKind::Scalar),
        YamlNode::Mapping(_) => Ok(NodeKind::Mapping),
        YamlNode::Sequence(_) => Ok(NodeKind::Sequence),
        YamlNode::Alias(_) | YamlNode::TaggedNode(_) => Err(CodeError::new(
            CodeErrorKind::InvalidSource,
            "aliases and tagged nodes are outside the Vibra source subset",
        )),
    }
}

fn form_from_yaml_node(node: &YamlNode) -> Result<Form, CodeError> {
    match node {
        YamlNode::Scalar(scalar) => Form::parse(&scalar.to_string()),
        YamlNode::Mapping(mapping) => mapping
            .iter()
            .map(|(key, value)| {
                let Form::String(key) = form_from_yaml_node(&key)? else {
                    return Err(CodeError::new(
                        CodeErrorKind::InvalidSource,
                        "Vibra structural mapping keys must be strings",
                    ));
                };
                Ok(super::Entry {
                    key,
                    value: form_from_yaml_node(&value)?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Form::Mapping),
        YamlNode::Sequence(sequence) => sequence
            .values()
            .map(|value| form_from_yaml_node(&value))
            .collect::<Result<Vec<_>, _>>()
            .map(Form::Sequence),
        YamlNode::Alias(_) | YamlNode::TaggedNode(_) => Err(CodeError::new(
            CodeErrorKind::InvalidSource,
            "aliases and tagged nodes are outside the Vibra source subset",
        )),
    }
}

pub(crate) fn content_hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.components().collect()
}

fn discover_sources(
    root: &FsPath,
    directory: &FsPath,
    sources: &mut Vec<(PathBuf, String)>,
) -> Result<(), CodeError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("read workspace directory {}: {error}", directory.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CodeError::new(
                CodeErrorKind::DocumentNotFound,
                format!("read workspace directory entry: {error}"),
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | ".worktrees" | "target")) {
                continue;
            }
            discover_sources(root, &path, sources)?;
            continue;
        }
        let display = path.to_string_lossy();
        if !display.ends_with(".vibra") && !display.ends_with(".vibra.yaml") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("discovered path is under root");
        let source = fs::read_to_string(&path).map_err(|error| {
            CodeError::new(
                CodeErrorKind::InvalidSource,
                format!("read {}: {error}", path.display()),
            )
            .at_document(relative.to_path_buf())
        })?;
        sources.push((relative.to_path_buf(), source));
    }
    Ok(())
}
