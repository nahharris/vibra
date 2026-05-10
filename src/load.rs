//! Load `.vibra` modules and resolve `$import` relative to each file or project namespace.

use crate::project;
use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonical path → parsed YAML root mapping.
#[derive(Debug)]
pub struct LoadedProgram {
    pub entry: PathBuf,
    pub modules: HashMap<PathBuf, Value>,
}

pub fn load_program(entry: &Path) -> Result<LoadedProgram> {
    let entry = fs::canonicalize(entry)
        .with_context(|| format!("cannot open entry module {}", entry.display()))?;
    let project = project::find_project_for_file(&entry)?;
    let entry = canonical_module_path(&entry)?;
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    load_recursive(&entry, project.as_ref(), &mut modules, &mut stack)?;
    Ok(LoadedProgram { entry, modules })
}

pub fn load_inline_program(base_dir: &Path, root: Value) -> Result<LoadedProgram> {
    let base_dir = fs::canonicalize(base_dir)
        .with_context(|| format!("resolve inline base directory {}", base_dir.display()))?;
    let entry = base_dir.join("__vibra_exec__.vibra");
    let project = project::find_project_for_file(&entry)?;
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    for import in module_imports(&entry, &root, project.as_ref())? {
        load_recursive(&import, project.as_ref(), &mut modules, &mut stack)?;
    }
    modules.insert(entry.clone(), root);
    Ok(LoadedProgram { entry, modules })
}

fn load_recursive(
    path: &Path,
    project: Option<&project::LoadedProject>,
    modules: &mut HashMap<PathBuf, Value>,
    stack: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = canonical_module_path(path)?;
    if modules.contains_key(&path) {
        return Ok(());
    }
    if stack.iter().any(|p| p.as_path() == path) {
        bail!(
            "import cycle detected (E-MOD-003): {} is already being loaded",
            path.display()
        );
    }
    stack.push(path.clone());

    let v = load_module_parts(&path)?;
    let imports = module_imports(&path, &v, project)?;

    for imp in imports {
        load_recursive(&imp, project, modules, stack)?;
    }

    modules.insert(path, v);
    stack.pop();
    Ok(())
}

fn module_imports(
    path: &Path,
    root: &Value,
    project: Option<&project::LoadedProject>,
) -> Result<Vec<PathBuf>> {
    let map = root
        .as_mapping()
        .with_context(|| format!("{}: root must be a mapping", path.display()))?;
    let parent = path
        .parent()
        .with_context(|| format!("{}: path has no parent directory", path.display()))?;
    let mut imports = Vec::new();
    for (k, val) in map {
        let key =
            key_as_str(k).with_context(|| format!("{}: keys must be strings", path.display()))?;
        let Some(sub) = val.as_mapping() else {
            continue;
        };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let s = imp
            .as_str()
            .with_context(|| format!("{}: $import must be a string path", path.display()))?;
        let resolved = if s.starts_with('@') {
            let project = project.with_context(|| {
                format!(
                    "{}: @ import `{s}` requires a project.vibra",
                    path.display()
                )
            })?;
            project::resolve_project_import(project, s)?
        } else {
            parent.join(s)
        };
        let resolved = fs::canonicalize(&resolved).with_context(|| {
            format!(
                "{}: cannot resolve import `{}` (from field `{}`)",
                path.display(),
                resolved.display(),
                key
            )
        })?;
        imports.push(canonical_module_path(&resolved)?);
    }
    Ok(imports)
}

fn load_module_parts(module_path: &Path) -> Result<Value> {
    let mut merged = Mapping::new();
    for part in module_part_paths(module_path)? {
        let text = fs::read_to_string(&part).with_context(|| format!("read {}", part.display()))?;
        let v: Value = serde_yaml::from_str(&text)
            .with_context(|| format!("YAML parse {}", part.display()))?;
        let map = v
            .as_mapping()
            .with_context(|| format!("{}: root must be a mapping", part.display()))?;
        for (key, value) in map {
            if merged.insert(key.clone(), value.clone()).is_some() {
                bail!(
                    "{}: duplicate module key `{}` across module parts",
                    part.display(),
                    key_as_str(key).unwrap_or("<non-string>")
                );
            }
        }
    }
    Ok(Value::Mapping(merged))
}

fn module_part_paths(module_path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![module_path.to_path_buf()];
    let Some(parent) = module_path.parent() else {
        return Ok(paths);
    };
    let Some(stem) = module_path.file_stem().and_then(|s| s.to_str()) else {
        return Ok(paths);
    };
    let prefix = format!("{stem}.");
    for entry in fs::read_dir(parent).with_context(|| format!("read {}", parent.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path == module_path {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_name.starts_with(&prefix) && is_vibra_file(&path) {
            paths.push(fs::canonicalize(&path)?);
        }
    }
    paths.sort();
    Ok(paths)
}

fn canonical_module_path(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return Ok(path);
    };
    if !is_vibra_file(&path) {
        return Ok(path);
    }
    let without_ext = file_name
        .strip_suffix(".vibra")
        .or_else(|| file_name.strip_suffix(".vibra.yaml"))
        .unwrap_or(file_name);
    let Some((base, _)) = without_ext.split_once('.') else {
        return Ok(path);
    };
    let candidate = path.with_file_name(format!("{base}.vibra"));
    if candidate.exists() {
        fs::canonicalize(candidate).with_context(|| format!("resolve base module for {file_name}"))
    } else {
        Ok(path)
    }
}

fn is_vibra_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".vibra") || s.ends_with(".vibra.yaml")
}

fn key_as_str(k: &Value) -> Result<&str> {
    k.as_str()
        .ok_or_else(|| anyhow::anyhow!("mapping key must be a string"))
}

pub fn map_get_str<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}
