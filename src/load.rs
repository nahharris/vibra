//! Load `.vibra` modules and resolve `$import` relative to each file (Python-like).

use anyhow::{bail, Context, Result};
use serde_yaml::Value;
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
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    load_recursive(&entry, &mut modules, &mut stack)?;
    Ok(LoadedProgram { entry, modules })
}

fn load_recursive(
    path: &Path,
    modules: &mut HashMap<PathBuf, Value>,
    stack: &mut Vec<PathBuf>,
) -> Result<()> {
    if modules.contains_key(path) {
        return Ok(());
    }
    if stack.iter().any(|p| p.as_path() == path) {
        bail!(
            "import cycle detected (E-MOD-003): {} is already being loaded",
            path.display()
        );
    }
    stack.push(path.to_path_buf());

    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let v: Value =
        serde_yaml::from_str(&text).with_context(|| format!("YAML parse {}", path.display()))?;
    let map = v
        .as_mapping()
        .with_context(|| format!("{}: root must be a mapping", path.display()))?;

    let parent = path
        .parent()
        .with_context(|| format!("{}: path has no parent directory", path.display()))?;

    let mut imports = Vec::new();
    for (k, val) in map {
        let key =
            key_as_str(k).with_context(|| format!("{}: keys must be strings", path.display()))?;
        if key.starts_with('-') {
            continue;
        }
        if let Some(sub) = val.as_mapping() {
            if let Some(imp) = map_get_str(sub, "$import") {
                let s = imp.as_str().with_context(|| {
                    format!("{}: $import must be a string path", path.display())
                })?;
                let resolved = parent.join(s);
                let resolved = fs::canonicalize(&resolved).with_context(|| {
                    format!(
                        "{}: cannot resolve import `{}` (from field `{}`)",
                        path.display(),
                        resolved.display(),
                        key
                    )
                })?;
                imports.push(resolved);
            }
        }
    }

    for imp in imports {
        load_recursive(&imp, modules, stack)?;
    }

    modules.insert(path.to_path_buf(), v);
    stack.pop();
    Ok(())
}

fn key_as_str(k: &Value) -> Result<&str> {
    k.as_str()
        .ok_or_else(|| anyhow::anyhow!("mapping key must be a string"))
}

pub fn map_get_str<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}
