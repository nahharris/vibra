//! Minimal serde-free project context for typed source consumers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::syntax::{self, Atom, Node, NodeKind};

pub const PROJECT_MANIFEST: &str = "project.vib";

#[derive(Debug, Clone)]
pub struct ProjectImportContext {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    namespaces: BTreeMap<String, PathBuf>,
}

pub fn discover_project_import_context(source_path: &Path) -> Result<Option<ProjectImportContext>> {
    let mut current = source_path
        .parent()
        .with_context(|| format!("{} has no parent directory", source_path.display()))?
        .to_path_buf();
    loop {
        let manifest_path = current.join(PROJECT_MANIFEST);
        if manifest_path.is_file() {
            return load_project_import_context(&manifest_path).map(Some);
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

pub fn resolve_project_import(context: &ProjectImportContext, import: &str) -> Result<PathBuf> {
    let rest = import
        .strip_prefix('@')
        .context("internal: @ import missing prefix")?;
    let (namespace, subpath) = rest
        .split_once('/')
        .with_context(|| format!("import `{import}` must use `@name/path`"))?;
    let root = context
        .namespaces
        .get(namespace)
        .with_context(|| format!("unknown @ import namespace `{namespace}`"))?;
    validate_relative_source_path(Path::new(subpath))?;
    Ok(root.join(subpath))
}

fn load_project_import_context(manifest_path: &Path) -> Result<ProjectImportContext> {
    let manifest_path = fs::canonicalize(manifest_path)
        .with_context(|| format!("resolve {}", manifest_path.display()))?;
    let root = manifest_path
        .parent()
        .context("project manifest has no parent")?
        .to_path_buf();
    let declarations = project_declarations(&manifest_path)?;
    let mut namespaces = BTreeMap::new();
    for declaration in declarations {
        let Some((head, values)) = headed(&declaration) else {
            continue;
        };
        match head {
            "target" => {
                let name = symbol(values.first().context("target requires a name")?)?;
                let labels = labels(&values[1..])?;
                let root_path = string(labels.get("root").context("target requires `root:`")?)?;
                namespaces.insert(name.to_string(), root.join(root_path));
            }
            "dependency" => {
                let name = symbol(values.first().context("dependency requires a name")?)?;
                let labels = labels(&values[1..])?;
                let dependency_root = labels
                    .get("path")
                    .map(|node| string(node).map(|path| root.join(path)))
                    .transpose()?
                    .unwrap_or_else(|| root.join("dep").join(name));
                namespaces.insert(
                    name.to_string(),
                    dependency_library_root(&dependency_root, name)?,
                );
            }
            _ => {}
        }
    }
    Ok(ProjectImportContext {
        root,
        manifest_path,
        namespaces,
    })
}

fn dependency_library_root(root: &Path, dependency_name: &str) -> Result<PathBuf> {
    let manifest_path = root.join(PROJECT_MANIFEST);
    if !manifest_path.is_file() {
        return Ok(root.to_path_buf());
    }
    let declarations = project_declarations(&manifest_path)?;
    let mut libraries = Vec::new();
    for declaration in declarations {
        let Some((head, values)) = headed(&declaration) else {
            continue;
        };
        if head != "target" {
            continue;
        }
        let name = symbol(values.first().context("target requires a name")?)?;
        let labels = labels(&values[1..])?;
        if !labels.get("kind").is_some_and(
            |node| matches!(&node.kind, NodeKind::Atom(Atom::Atom(kind)) if kind == "lib"),
        ) {
            continue;
        }
        let target_root = string(labels.get("root").context("target requires `root:`")?)?;
        libraries.push((name.to_string(), target_root.to_string()));
    }
    let selected = libraries
        .iter()
        .find(|(name, _)| name == dependency_name)
        .or_else(|| (libraries.len() == 1).then(|| &libraries[0]))
        .with_context(|| {
            format!(
                "dependency `{dependency_name}` must expose a matching or single library target"
            )
        })?;
    Ok(root.join(&selected.1))
}

fn project_declarations(manifest_path: &Path) -> Result<Vec<Node>> {
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let document =
        syntax::parse(&source).with_context(|| format!("parse {}", manifest_path.display()))?;
    let root = semantic_nodes(&document.nodes)
        .next()
        .context("project manifest is empty")?;
    let (head, values) = headed(root).context("project manifest root must be a list")?;
    if head != "project" {
        bail!("project manifest root must be `(project ...)`");
    }
    // The nodes are borrowed from `document`, so return owned clones.
    Ok(values.into_iter().cloned().collect())
}

fn semantic_nodes(nodes: &[Node]) -> impl Iterator<Item = &Node> {
    nodes
        .iter()
        .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
}

fn headed(node: &Node) -> Option<(&str, Vec<&Node>)> {
    let NodeKind::List(nodes) = &node.kind else {
        return None;
    };
    let mut nodes = semantic_nodes(nodes);
    let head = symbol(nodes.next()?).ok()?;
    Some((head, nodes.collect()))
}

fn labels<'a>(nodes: &[&'a Node]) -> Result<BTreeMap<&'a str, &'a Node>> {
    let mut result = BTreeMap::new();
    let mut index = 0;
    while index < nodes.len() {
        let NodeKind::Atom(Atom::Label(label)) = &nodes[index].kind else {
            index += 1;
            continue;
        };
        let value = nodes
            .get(index + 1)
            .with_context(|| format!("label `{label}:` requires a value"))?;
        result.insert(label.as_str(), *value);
        index += 2;
    }
    Ok(result)
}

fn symbol(node: &Node) -> Result<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Symbol(value)) => Ok(value),
        _ => bail!("expected symbol"),
    }
}

fn string(node: &Node) -> Result<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::String(value)) => Ok(value),
        _ => bail!("expected string"),
    }
}

fn validate_relative_source_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!("@ import path must be a relative source path without `..`");
    }
    Ok(())
}
