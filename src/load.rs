//! Load `.vibra` modules and resolve imports relative to each file or project namespace.
//!
//! Source parsing goes through the S-expression frontend
//! (`crate::frontend::load_surface_program`), which produces a typed
//! `SurfaceProgram`: imports resolved, module parts merged, macros expanded,
//! and compile-time `embed`/`template` data expanded. This module then
//! adapts that typed program into the internal legacy value shape
//! `src/lower.rs` already consumes, via `crate::surface_adapter` -- the
//! internal migration bridge documented in the "Amendment: internal typed-
//! AST-to-`Value` migration bridge" section of
//! `docs/decisions/s-expression-language.md`.
//!
//! There is no second source parser and no content-based dialect selection:
//! `.vibra` source is always read as S-expression, and
//! `frontend::canonical_module_path` rejects `.vibra.yaml` and any other
//! extension outright (`E-SYN-012`) before this module ever sees it.

use crate::ast;
use crate::frontend::{self, SourceModule, SurfaceProgram};
use crate::legacy_value::{Mapping, Value};
use crate::surface_adapter::{self, LocalSignatures};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Canonical path → parsed program root, in the legacy `Value` shape
/// `src/lower.rs` consumes (see module docs for why it is still
/// an internal compatibility value rather than the typed AST).
#[derive(Debug)]
pub struct LoadedProgram {
    pub entry: PathBuf,
    pub modules: HashMap<PathBuf, Value>,
    pub module_parts: HashMap<PathBuf, Vec<PathBuf>>,
    /// Canonical compile-time input path to raw-content SHA-256. This is part
    /// of the compiler fingerprint even when two inputs produce equal values.
    pub embedded_files: BTreeMap<PathBuf, String>,
}

/// Compiler flags that select conditional module parts.
///
/// A file named `name.flag.vibra` contributes to `name.vibra` only when
/// `flag` is enabled. Multiple suffix segments are conjunctive, so
/// `name.unix.debug.vibra` requires both `unix` and `debug`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompilationFlags {
    enabled: BTreeSet<String>,
}

impl CompilationFlags {
    pub fn new(flags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            enabled: flags.into_iter().map(Into::into).collect(),
        }
    }

    pub fn try_new(flags: impl IntoIterator<Item = impl Into<String>>) -> Result<Self> {
        let flags = Self::new(flags);
        flags.validate()?;
        Ok(flags)
    }

    pub fn contains(&self, flag: &str) -> bool {
        self.enabled.contains(flag)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.enabled.iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.enabled.is_empty()
    }

    fn validate(&self) -> Result<()> {
        for flag in &self.enabled {
            if !is_compilation_flag(flag) {
                bail!("E-FLAG-001: invalid compilation flag `{flag}`; expected a kebab-case name");
            }
        }
        Ok(())
    }
}

pub fn is_compilation_flag(flag: &str) -> bool {
    flag.bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && flag.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Compatibility wrapper for callers that still consume the legacy `Value` tree.
#[doc(hidden)]
pub fn load_program(entry: &Path) -> Result<LoadedProgram> {
    load_legacy_yaml_program(entry, &CompilationFlags::default())
}

/// Compatibility wrapper for callers that still consume the legacy `Value` tree.
#[doc(hidden)]
pub fn load_program_with_flags(entry: &Path, flags: &CompilationFlags) -> Result<LoadedProgram> {
    load_legacy_yaml_program(entry, flags)
}

/// Load and adapt a real `.vibra` entry module (and its transitive imports)
/// into the legacy `Value` shape `src/lower.rs` consumes.
///
/// The name is a historical artifact of `LoadedProgram`'s original map-based
/// implementation, kept because `src/lower.rs`'s roughly 240 accessor call
/// sites and every caller of this function are the seam that shape was built
/// around (see `crate::lower`, `crate::execute`, `crate::wasm_backend`).
/// Nothing in this function parses YAML: source is read exclusively through
/// [`frontend::load_surface_program`]'s S-expression frontend.
#[doc(hidden)]
pub fn load_legacy_yaml_program(entry: &Path, flags: &CompilationFlags) -> Result<LoadedProgram> {
    flags.validate()?;
    let surface = frontend::load_surface_program(entry, flags)?;
    let modules = convert_surface_program(&surface)?;
    Ok(LoadedProgram {
        entry: surface.entry,
        modules,
        module_parts: surface.module_parts.into_iter().collect(),
        embedded_files: surface.embedded_files,
    })
}

/// Load only the entry module (including its module parts) for `$test`
/// discovery. This deliberately does not resolve imports, so a test that
/// expects a load failure can still be selected and delegated to its worker.
/// Entry source is parsed and validated (reader + typed surface) but never
/// macro-expanded or compile-time-expanded, matching the legacy loader's own
/// discovery-only guarantees.
///
/// Only [`crate::surface_adapter::test_discovery_value`]'s metadata is
/// materialized -- test bodies are never converted, since discovery never
/// resolves imports and therefore has no cross-module signature index to
/// convert a call against.
pub fn load_entry_module_for_test_discovery(entry: &Path) -> Result<(PathBuf, Value)> {
    let (path, source_module) = frontend::load_surface_entry_for_test_discovery(entry)?;
    let module = merge_module_parts(&source_module);
    let mut map = Mapping::new();
    for form in &module.forms {
        let cases = match form {
            ast::TopLevel::Test(test) => vec![(test.name.value.clone(), test)],
            ast::TopLevel::TestScenario(scenario) => scenario
                .cases
                .iter()
                .map(|test| {
                    (
                        format!("{}::{}", scenario.name.value, test.name.value),
                        test,
                    )
                })
                .collect(),
            _ => continue,
        };
        for (name, test) in cases {
            let (_, value) = surface_adapter::test_discovery_value(test)?;
            if map.insert(Value::String(name.clone()), value).is_some() {
                bail!(
                    "E-MOD-002: duplicate top-level symbol `{name}` across module parts of {}",
                    path.display()
                );
            }
        }
    }
    Ok((path, Value::Mapping(map)))
}

/// Compatibility wrapper for callers that still consume the legacy `Value` tree.
#[doc(hidden)]
pub fn load_inline_program(base_dir: &Path, root: Value) -> Result<LoadedProgram> {
    load_legacy_yaml_inline_program(base_dir, root)
}

/// Parse one inline expression with Vibra's typed S-expression reader, then
/// adapt it to the legacy lowering shape used by `lower_exec_expr`.
pub fn parse_inline_exec_expression(source: &str) -> Result<Value> {
    let document = crate::syntax::parse(source).context("parse exec expression")?;
    let nodes = document
        .nodes
        .iter()
        .filter(|node| !matches!(node.kind, crate::syntax::NodeKind::Comment(_)))
        .collect::<Vec<_>>();
    let [node] = nodes.as_slice() else {
        bail!("exec expression must contain exactly one S-expression");
    };
    let expression =
        crate::ast::lower_expression_node_with_id(node, crate::ast::DocumentId::ANONYMOUS)
            .context("validate exec expression")?;
    crate::surface_adapter::inline_expression_to_value(&expression).context("adapt exec expression")
}

/// `vibra exec`'s inline program seam. `root` is a synthetic in-memory module
/// (its `$import` entries built from `--import alias=path` flags), not a real
/// `.vibra` file, so it needs no S-expression parsing of its own -- it is
/// inserted into the result verbatim. Its imports, however, are real files
/// and go through the same typed S-expression frontend as every other
/// module.
#[doc(hidden)]
pub fn load_legacy_yaml_inline_program(base_dir: &Path, root: Value) -> Result<LoadedProgram> {
    let base_dir = std::fs::canonicalize(base_dir)
        .with_context(|| format!("resolve inline base directory {}", base_dir.display()))?;
    let entry = base_dir.join("__vibra_exec__.vibra");
    let flags = CompilationFlags::default();
    let roots = inline_import_roots(&entry, &root)?;
    let surface = frontend::load_surface_program_multi_root(&roots, &flags)?;
    let mut modules = convert_surface_program(&surface)?;
    modules.insert(entry.clone(), root);
    let mut module_parts: HashMap<PathBuf, Vec<PathBuf>> =
        surface.module_parts.into_iter().collect();
    module_parts.insert(entry.clone(), vec![entry.clone()]);
    Ok(LoadedProgram {
        entry,
        modules,
        module_parts,
        embedded_files: surface.embedded_files,
    })
}

/// Resolve every `$import` in the synthetic inline root to a real, canonical
/// `.vibra` path, exactly as a real module's imports would resolve (relative
/// paths against the synthetic entry's directory, `@name/path` against the
/// project namespace if `base_dir` sits inside one).
fn inline_import_roots(entry: &Path, root: &Value) -> Result<Vec<PathBuf>> {
    let map = root
        .as_mapping()
        .context("internal: inline exec root must be a mapping")?;
    let parent = entry
        .parent()
        .context("internal: inline exec entry has no parent directory")?;
    let project = crate::project_context::discover_project_import_context(entry)?;
    let mut roots = Vec::new();
    for (_, definition) in map {
        let Some(import) = definition
            .as_mapping()
            .and_then(|definition| map_get_str(definition, "$import"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let resolved = if import.starts_with('@') {
            let project = project.as_ref().with_context(|| {
                format!(
                    "{}: @ import `{import}` requires a project.vibra",
                    entry.display()
                )
            })?;
            crate::project_context::resolve_project_import(project, import)?
        } else {
            parent.join(import)
        };
        roots.push(resolved);
    }
    Ok(roots)
}

/// Adapt an already-parsed, already-macro-expanded, already-compile-time-
/// expanded typed [`SurfaceProgram`] into the legacy `Value` shape
/// `src/lower.rs` consumes, via `crate::surface_adapter`.
///
/// Each module's own [`LocalSignatures`] only needs that module's own
/// declarations (`crate::surface_adapter::collect_local_signatures` never
/// recurses into imports), so every module's signatures can be collected
/// independently before any module is converted; conversion order does not
/// need to be topological.
fn convert_surface_program(surface: &SurfaceProgram) -> Result<HashMap<PathBuf, Value>> {
    let mut merged: BTreeMap<PathBuf, ast::Module> = BTreeMap::new();
    let mut local_signatures: BTreeMap<PathBuf, LocalSignatures> = BTreeMap::new();
    for (path, source_module) in &surface.modules {
        let module = merge_module_parts(source_module);
        let sig = surface_adapter::collect_local_signatures(&module)
            .with_context(|| format!("collect signatures for {}", path.display()))?;
        local_signatures.insert(path.clone(), sig);
        merged.insert(path.clone(), module);
    }
    let empty_imports = BTreeMap::new();
    let mut modules = HashMap::with_capacity(merged.len());
    for (path, module) in &merged {
        let own_stem = frontend::module_self_alias(path).unwrap_or_default();
        let own_local = &local_signatures[path];
        let import_map = surface.imports.get(path).unwrap_or(&empty_imports);
        let mut imports = Vec::with_capacity(import_map.len());
        for (alias, target) in import_map {
            let sig = local_signatures.get(target).with_context(|| {
                format!(
                    "internal: {} imports {} as `{alias}`, but it was not loaded",
                    path.display(),
                    target.display()
                )
            })?;
            imports.push((alias.clone(), sig.clone()));
        }
        let resolved = surface_adapter::resolve_signatures(&own_stem, own_local, &imports);
        let value = surface_adapter::module_to_value(module, &resolved)
            .with_context(|| format!("adapt {} to the legacy compiler shape", path.display()))?;
        modules.insert(path.clone(), value);
    }
    Ok(modules)
}

/// Merge a module's physical parts (base file plus any selected
/// `name.flag.vibra` conditional parts) into one typed [`ast::Module`], the
/// unit [`crate::surface_adapter`] converts. Duplicate top-level symbols
/// across parts are already rejected by [`frontend::load_surface_program`]
/// (`E-MOD-002`) before this ever runs.
fn merge_module_parts(source_module: &SourceModule) -> ast::Module {
    let mut forms = Vec::new();
    for part in &source_module.parts {
        forms.extend(part.module.forms.iter().cloned());
    }
    let first = source_module
        .parts
        .first()
        .expect("a source module always has at least one physical part");
    ast::Module {
        forms,
        span: first.module.span,
        document_id: first.module.document_id,
    }
}

pub fn map_get_str<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(&Value::String(key.into()))
}
