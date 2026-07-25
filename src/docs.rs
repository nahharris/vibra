//! Source-level documentation discovery for CLI and editor tooling.

use crate::{load, project};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Documentation {
    pub symbol: String,
    pub kind: String,
    pub documentation: String,
    pub module: PathBuf,
}

/// Resolve an entry module from either a source file or a project directory.
pub fn resolve_entry(path: &Path, target: Option<&str>) -> Result<PathBuf> {
    if path.is_file()
        && path.file_name().and_then(|name| name.to_str()) != Some(project::MANIFEST_FILE)
    {
        return Ok(path.to_path_buf());
    }
    let loaded = project::load_project(path)?;
    let targets = loaded
        .manifest
        .targets
        .libs
        .iter()
        .chain(loaded.manifest.targets.bins.iter())
        .collect::<Vec<_>>();
    let selected = match target {
        Some(name) => targets
            .iter()
            .find(|candidate| candidate.name == name)
            .copied()
            .with_context(|| format!("project target `{name}` does not exist"))?,
        None if targets.len() == 1 => targets[0],
        None => bail!("project has multiple targets; select one with `--target <name>`"),
    };
    Ok(loaded.root.join(&selected.root).join(&selected.entry))
}

/// Collect documented modules and symbols visible from an entry module.
pub fn collect(entry: &Path) -> Result<Vec<Documentation>> {
    collect_with_flags(entry, &load::CompilationFlags::default())
}

pub fn collect_with_flags(
    entry: &Path,
    flags: &load::CompilationFlags,
) -> Result<Vec<Documentation>> {
    let program = load::load_program_with_flags(entry, flags)?;
    let project = project::find_project_for_file(&program.entry)?;
    let mut docs = BTreeMap::new();
    let mut visited = HashSet::new();
    collect_module(
        &program.entry,
        "",
        &program.modules,
        project.as_ref(),
        &mut visited,
        &mut docs,
    )?;

    if let Some(project) = project {
        if let Some(documentation) = &project.manifest.package.doc {
            docs.insert(
                format!("\0{}", project.manifest.package.name),
                Documentation {
                    symbol: project.manifest.package.name,
                    kind: "package".into(),
                    documentation: documentation.clone(),
                    module: project.manifest_path,
                },
            );
        }
    }
    Ok(docs.into_values().collect())
}

fn collect_module(
    module: &Path,
    alias: &str,
    modules: &std::collections::HashMap<PathBuf, Value>,
    project: Option<&project::LoadedProject>,
    visited: &mut HashSet<(PathBuf, String)>,
    docs: &mut BTreeMap<String, Documentation>,
) -> Result<()> {
    if !visited.insert((module.to_path_buf(), alias.to_string())) {
        return Ok(());
    }
    let root = modules
        .get(module)
        .with_context(|| format!("loaded module {} is missing", module.display()))?;
    let mapping = root
        .as_mapping()
        .with_context(|| format!("{}: root must be a mapping", module.display()))?;

    if let Some(documentation) = string_field(mapping, "=doc") {
        let symbol = if alias.is_empty() {
            module_name(module)
        } else {
            alias.to_string()
        };
        docs.insert(
            symbol.clone(),
            Documentation {
                symbol,
                kind: "module".into(),
                documentation: documentation.to_string(),
                module: module.to_path_buf(),
            },
        );
    }

    for (key, value) in mapping {
        let Some(name) = key.as_str() else { continue };
        if name.starts_with('=') {
            continue;
        }
        let Some(definition) = value.as_mapping() else {
            continue;
        };
        if let Some(import) = string_field(definition, "$import") {
            let imported = resolve_import(module, import, project)?;
            let imported_alias = qualify(alias, name);
            collect_module(&imported, &imported_alias, modules, project, visited, docs)?;
            continue;
        }
        let symbol = qualify(alias, name);
        collect_definition(module, &symbol, definition, docs);
    }
    Ok(())
}

fn collect_definition(
    module: &Path,
    symbol: &str,
    definition: &Mapping,
    docs: &mut BTreeMap<String, Documentation>,
) {
    if let Some(documentation) = string_field(definition, "=doc") {
        docs.insert(
            symbol.to_string(),
            Documentation {
                symbol: symbol.to_string(),
                kind: definition_kind(definition).to_string(),
                documentation: documentation.to_string(),
                module: module.to_path_buf(),
            },
        );
    }
    if let Some(defs) = definition
        .get(Value::String("=defs".into()))
        .and_then(Value::as_mapping)
    {
        for (key, value) in defs {
            if let (Some(name), Some(definition)) = (key.as_str(), value.as_mapping()) {
                collect_definition(module, &format!("{symbol}.{name}"), definition, docs);
            }
        }
    }
}

fn resolve_import(
    module: &Path,
    import: &str,
    project: Option<&project::LoadedProject>,
) -> Result<PathBuf> {
    let path = if import.starts_with('@') {
        project::resolve_project_import(
            project.context("@ import requires a project.vibra")?,
            import,
        )?
    } else {
        module
            .parent()
            .context("module path has no parent")?
            .join(import)
    };
    fs::canonicalize(&path).with_context(|| format!("resolve import `{import}`"))
}

fn string_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a str> {
    mapping
        .get(Value::String(key.into()))
        .and_then(Value::as_str)
}

fn definition_kind(definition: &Mapping) -> &'static str {
    for (form, kind) in [
        ("$function", "function"),
        ("$type", "type"),
        ("$enum", "enum"),
        ("$interface", "interface"),
        ("$literal", "constant"),
        ("$macro", "macro"),
        ("$test", "test"),
    ] {
        if definition.contains_key(Value::String(form.into())) {
            return kind;
        }
    }
    "symbol"
}

fn qualify(alias: &str, name: &str) -> String {
    if alias.is_empty() {
        name.to_string()
    } else {
        format!("{alias}.{name}")
    }
}

fn module_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("module")
        .trim_end_matches(".yaml")
        .trim_end_matches(".vibra")
        .to_string()
}
