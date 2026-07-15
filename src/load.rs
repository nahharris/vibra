//! Load `.vibra` modules and resolve `$import` relative to each file or project namespace.

use crate::code::SourceDatabase;
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
    pub sources: SourceDatabase,
    pub module_parts: HashMap<PathBuf, Vec<PathBuf>>,
}

pub fn load_program(entry: &Path) -> Result<LoadedProgram> {
    let entry = fs::canonicalize(entry)
        .with_context(|| format!("cannot open entry module {}", entry.display()))?;
    let project = project::find_project_for_file(&entry)?;
    let entry = canonical_module_path(&entry)?;
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    load_recursive(&entry, project.as_ref(), &mut modules, &mut stack)?;
    let (sources, module_parts) = source_database_for_modules(&entry, modules.keys(), None)?;
    let mut modules = rebuild_modules_from_sources(modules.keys(), &sources, &module_parts)?;
    normalize_ignored_annotations(&mut modules)?;
    let modules = crate::macro_expand::expand_program_modules(&entry, modules)?;
    validate_direct_import_aliases(&modules)?;
    Ok(LoadedProgram {
        entry,
        modules,
        sources,
        module_parts,
    })
}

/// Load only the entry module (including its module parts) for `$test`
/// discovery. This deliberately does not resolve imports, so a test that
/// expects a load failure can still be selected and delegated to its worker.
/// Entry YAML remains fully parsed and validated by the normal module loader.
pub fn load_entry_module_for_test_discovery(entry: &Path) -> Result<(PathBuf, Value)> {
    let entry = fs::canonicalize(entry)
        .with_context(|| format!("cannot open entry module {}", entry.display()))?;
    let entry = canonical_module_path(&entry)?;
    let root = load_module_parts(&entry)?;
    Ok((entry, root))
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
    modules.insert(entry.clone(), root.clone());
    let inline_source = serde_yaml::to_string(&root).context("serialize inline Vibra program")?;
    let (sources, module_parts) =
        source_database_for_modules(&entry, modules.keys(), Some((&entry, inline_source)))?;
    let mut modules = rebuild_modules_from_sources(modules.keys(), &sources, &module_parts)?;
    normalize_ignored_annotations(&mut modules)?;
    let modules = crate::macro_expand::expand_program_modules(&entry, modules)?;
    validate_direct_import_aliases(&modules)?;
    Ok(LoadedProgram {
        entry,
        modules,
        sources,
        module_parts,
    })
}

fn normalize_ignored_annotations(modules: &mut HashMap<PathBuf, Value>) -> Result<()> {
    for (path, module) in modules {
        crate::annotations::validate(module)
            .with_context(|| format!("validate annotations in {}", path.display()))?;
        crate::annotations::strip(module);
    }
    Ok(())
}

fn source_database_for_modules<'a>(
    entry: &Path,
    modules: impl Iterator<Item = &'a PathBuf>,
    inline: Option<(&Path, String)>,
) -> Result<(SourceDatabase, HashMap<PathBuf, Vec<PathBuf>>)> {
    let mut source_pairs = Vec::new();
    let mut module_parts = HashMap::new();
    for module in modules {
        if inline.as_ref().is_some_and(|(path, _)| *path == module) {
            continue;
        }
        let parts = module_part_paths(module)?;
        for part in &parts {
            let source =
                fs::read_to_string(part).with_context(|| format!("read {}", part.display()))?;
            source_pairs.push((part.clone(), source));
        }
        module_parts.insert(module.clone(), parts);
    }
    if let Some((path, source)) = inline {
        source_pairs.push((path.to_path_buf(), source));
        module_parts.insert(path.to_path_buf(), vec![path.to_path_buf()]);
    }
    let root = entry
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let sources = SourceDatabase::from_sources(root, source_pairs)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok((sources, module_parts))
}

fn rebuild_modules_from_sources<'a>(
    modules: impl Iterator<Item = &'a PathBuf>,
    sources: &SourceDatabase,
    module_parts: &HashMap<PathBuf, Vec<PathBuf>>,
) -> Result<HashMap<PathBuf, Value>> {
    let mut rebuilt = HashMap::new();
    for module in modules {
        let mut merged = Mapping::new();
        let parts = module_parts
            .get(module)
            .with_context(|| format!("missing source origins for {}", module.display()))?;
        for part in parts {
            let source = sources.document(part).map_err(|error| {
                anyhow::anyhow!("read source snapshot {}: {error}", part.display())
            })?;
            let value: Value = serde_yaml::from_str(source.source())
                .with_context(|| format!("YAML parse {}", part.display()))?;
            let mapping = value
                .as_mapping()
                .with_context(|| format!("{}: root must be a mapping", part.display()))?;
            for (key, value) in mapping {
                if merged.insert(key.clone(), value.clone()).is_some() {
                    bail!(
                        "{}: duplicate module key `{}` across module parts",
                        part.display(),
                        key_as_str(key).unwrap_or("<non-string>")
                    );
                }
            }
        }
        rebuilt.insert(module.clone(), Value::Mapping(merged));
    }
    Ok(rebuilt)
}

fn validate_direct_import_aliases(modules: &HashMap<PathBuf, Value>) -> Result<()> {
    let known_import_aliases = modules
        .values()
        .filter_map(Value::as_mapping)
        .flat_map(|map| {
            map.iter().filter_map(|(key, value)| {
                value
                    .as_mapping()
                    .and_then(|definition| map_get_str(definition, "$import"))
                    .and_then(|_| key.as_str())
            })
        })
        .collect::<std::collections::HashSet<_>>();

    for (path, root) in modules {
        let map = root
            .as_mapping()
            .with_context(|| format!("{}: root must be a mapping", path.display()))?;
        let direct_imports = map
            .iter()
            .filter_map(|(key, value)| {
                value
                    .as_mapping()
                    .and_then(|definition| map_get_str(definition, "$import"))
                    .and_then(|_| key.as_str())
            })
            .collect::<std::collections::HashSet<_>>();
        // Same-module `$symbol.nested` references (e.g. local enum constructors) are
        // allowed; only import-alias qualifiers must be declared via `$import`.
        let local_symbols = map
            .keys()
            .filter_map(Value::as_str)
            .filter(|symbol| !direct_imports.contains(symbol))
            .collect::<std::collections::HashSet<_>>();
        let self_alias = module_self_alias(path);

        validate_value_aliases(
            root,
            path,
            &known_import_aliases,
            &direct_imports,
            &local_symbols,
            self_alias.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_value_aliases(
    value: &Value,
    path: &Path,
    known_import_aliases: &std::collections::HashSet<&str>,
    direct_imports: &std::collections::HashSet<&str>,
    local_symbols: &std::collections::HashSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    match value {
        Value::Mapping(map) => {
            for (key, value) in map {
                if let Some(key) = key.as_str() {
                    validate_reference_alias(
                        key,
                        path,
                        known_import_aliases,
                        direct_imports,
                        local_symbols,
                        self_alias,
                    )?;
                    // `=doc` is plain documentation text, not an expression surface.
                    if key == "=doc" {
                        continue;
                    }
                }
                validate_value_aliases(
                    value,
                    path,
                    known_import_aliases,
                    direct_imports,
                    local_symbols,
                    self_alias,
                )?;
            }
        }
        Value::Sequence(items) => {
            for item in items {
                validate_value_aliases(
                    item,
                    path,
                    known_import_aliases,
                    direct_imports,
                    local_symbols,
                    self_alias,
                )?;
            }
        }
        Value::String(value) => validate_reference_alias(
            value,
            path,
            known_import_aliases,
            direct_imports,
            local_symbols,
            self_alias,
        )?,
        _ => {}
    }
    Ok(())
}

fn validate_reference_alias(
    reference: &str,
    path: &Path,
    known_import_aliases: &std::collections::HashSet<&str>,
    direct_imports: &std::collections::HashSet<&str>,
    local_symbols: &std::collections::HashSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    let Some(reference) = reference.strip_prefix('$') else {
        return Ok(());
    };
    let Some((alias, _)) = reference.split_once('.') else {
        return Ok(());
    };
    if direct_imports.contains(alias)
        || local_symbols.contains(alias)
        || self_alias == Some(alias)
        || matches!(alias, "args" | "const" | "grants" | "policy" | "self")
        || !known_import_aliases.contains(alias)
    {
        return Ok(());
    }
    bail!(
        "E-MOD-004: module {} uses import alias `{alias}` without declaring it directly",
        path.display()
    )
}

fn module_self_alias(path: &Path) -> Option<String> {
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
        crate::yaml_subset::validate_yaml_subset_or_err(&text, &part)?;
        let v = parse_module_yaml(&text, &part)?;
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

fn parse_module_yaml(text: &str, path: &Path) -> Result<Value> {
    serde_yaml::from_str(text).map_err(|error| {
        let message = error.to_string();
        if text.contains("expect-error:") {
            for field in ["phase", "code", "message-contains"] {
                if message.contains(&format!("duplicate entry with key \"{field}\"")) {
                    return anyhow::anyhow!(
                        "E-TEST-001: duplicate `expect-error.{field}` key in {}",
                        path.display()
                    );
                }
            }
        }
        anyhow::anyhow!("YAML parse {}: {error}", path.display())
    })
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
