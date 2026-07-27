//! Project manifest, scaffold, dependency sync, and import validation.

use anyhow::{bail, Context, Result};
use git2::{build::CheckoutBuilder, Oid, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MANIFEST_FILE: &str = "project.vibra";
pub const LOCK_FILE: &str = "vibra.lock.json";
pub const LEGACY_LOCK_FILE: &str = "project.lock.vibra";
pub const STDLIB_GIT: &str = "https://github.com/nahharris/vibra-stdlib.git";
pub const STDLIB_REV: &str = "6b9fa5838e4f4122ff141e13a5ef737e99955dad";

#[derive(Debug, Clone, Copy)]
pub enum InitTemplate {
    Bin,
    Lib,
    Workspace,
}

#[derive(Debug, Deserialize)]
pub struct ProjectManifest {
    #[serde(rename = "manifest-version")]
    pub manifest_version: u32,
    pub package: Package,
    #[serde(default)]
    pub targets: Targets,
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    #[serde(default, rename = "plugin-interfaces")]
    pub plugin_interfaces: BTreeMap<String, PluginInterface>,
}

#[derive(Debug, Deserialize)]
pub struct PluginInterface {
    pub functions: BTreeMap<String, PluginFunction>,
}

#[derive(Debug, Deserialize)]
pub struct PluginFunction {
    #[serde(default)]
    pub params: Vec<String>,
    pub result: String,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default, rename = "=doc")]
    pub doc: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Targets {
    #[serde(default)]
    pub libs: Vec<Target>,
    #[serde(default)]
    pub bins: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub name: String,
    pub root: PathBuf,
    pub entry: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub path: Option<PathBuf>,
    pub git: Option<String>,
    pub rev: Option<String>,
    /// Package-relative static WebAssembly library exposed as `@<alias>` to
    /// `$wasm.import.module`. The artifact is validated by `vibra check`.
    pub wasm: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProjectLock {
    lock_version: u32,
    packages: Vec<LockedPackage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct LockedPackage {
    name: String,
    identity: String,
    git: String,
    rev: String,
    tree_sha256: String,
    vendor_path: String,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

pub struct LoadedProject {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ProjectManifest,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ReleaseMetadata {
    format_version: u32,
    stdlib_git: String,
    stdlib_rev: String,
    stdlib_sha256: String,
}

pub fn init_project(path: &Path, template: InitTemplate) -> Result<()> {
    if path.exists() && fs::read_dir(path)?.next().is_some() {
        bail!(
            "project directory `{}` already exists and is not empty",
            path.display()
        );
    }
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    fs::create_dir_all(path.join("dep")).with_context(|| "create dep directory")?;
    copy_clean_repository_tree(&locate_stdlib_package()?, &path.join("dep/std"))
        .context("copy stdlib into dep/std")?;
    fs::write(
        path.join("dep/std").join(MANIFEST_FILE),
        "(project\n  (package \"std\" \"0.1.0\")\n  (target std kind: lib root: \"src\" entry: \"lib.vibra\"))\n",
    )?;

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("project path must have a directory name")?;
    let name = normalize_name(name)?;
    match template {
        InitTemplate::Bin => write_bin_template(path, &name)?,
        InitTemplate::Lib => write_lib_template(path, &name)?,
        InitTemplate::Workspace => write_workspace_template(path, &name)?,
    }
    write_seeded_stdlib_lock(path)?;
    Ok(())
}

pub fn sync_project(path: &Path) -> Result<()> {
    let project = load_project(path)?;
    validate_manifest_shape(&project)?;
    let staging = tempfile::tempdir_in(&project.root).context("create dependency staging area")?;
    let staged_dep = staging.path().join("dep");
    fs::create_dir_all(&staged_dep)?;
    let mut packages = Vec::new();
    let mut stack = Vec::new();
    let mut stdlib_rev = None;
    for name in sorted_dependency_names(&project.manifest.dependencies) {
        let dep = &project.manifest.dependencies[name];
        if dep.git.is_some() {
            vendor_git_dependency(
                name,
                dep,
                &staged_dep.join(name),
                &format!("dep/{name}"),
                &mut packages,
                &mut stack,
                &mut stdlib_rev,
            )?;
        }
    }
    packages.sort_by(|a, b| a.vendor_path.cmp(&b.vendor_path));
    let lock = ProjectLock {
        lock_version: 1,
        packages,
    };
    let dep_root = project.root.join("dep");
    if dep_root.exists() {
        fs::remove_dir_all(&dep_root).context("replace existing vendored dependency graph")?;
    }
    fs::rename(&staged_dep, &dep_root).context("install vendored dependency graph")?;
    let lock_text = canonical_json(&lock).context("serialize dependency lock")?;
    fs::write(project.root.join(LOCK_FILE), lock_text).context("write dependency lock")?;
    Ok(())
}

pub fn check_project(path: &Path) -> Result<()> {
    check_project_with_flags(path, &crate::load::CompilationFlags::default())
}

pub fn check_project_with_flags(path: &Path, flags: &crate::load::CompilationFlags) -> Result<()> {
    let project = load_project(path)?;
    validate_manifest_shape(&project)?;
    validate_dependency_paths(&project)?;
    validate_project_lock(&project)?;
    validate_external_wasm_imports(&project)?;
    validate_target_imports(&project, flags)?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<LoadedProject> {
    let manifest_path = resolve_manifest_path(path)?;
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest =
        parse_manifest(&text).with_context(|| format!("parse {}", manifest_path.display()))?;
    let root = manifest_path
        .parent()
        .context("manifest path has no parent")?
        .to_path_buf();
    Ok(LoadedProject {
        root,
        manifest_path,
        manifest,
    })
}

fn parse_manifest(source: &str) -> Result<ProjectManifest> {
    if source.trim_start().starts_with("manifest-version:") {
        bail!("E-PROJECT-001: legacy YAML project manifests are unsupported; rewrite `project.vibra` as `(project ...)`");
    }
    let document =
        crate::syntax::parse(source).map_err(|error| anyhow::anyhow!("E-PROJECT-001: {error}"))?;
    let roots: Vec<_> = document
        .nodes
        .iter()
        .filter(|node| !matches!(node.kind, crate::syntax::NodeKind::Comment(_)))
        .collect();
    if roots.len() != 1 {
        bail!("E-PROJECT-001: manifest must contain exactly one `(project ...)` root");
    }
    let root = list(roots[0], "project root")?;
    if symbol(root.first(), "project root")? != "project" {
        bail!("E-PROJECT-001: manifest root must be `(project ...)`");
    }
    let mut package = None;
    let mut targets = Targets::default();
    let mut dependencies = HashMap::new();
    let mut plugin_interfaces = BTreeMap::new();
    for child in forms(&root[1..]) {
        let form = list(child, "project child")?;
        match symbol(form.first(), "project child")? {
            "package" => {
                if form.len() < 3 {
                    bail!("E-PROJECT-001: package requires name and version");
                }
                if package.is_some() {
                    bail!("E-PROJECT-001: duplicate `(package ...)` form");
                }
                let attrs = attributes(&form[3..], &["doc"], &[], "package")?;
                let doc = attrs
                    .get("doc")
                    .map(|node| string(node, "package doc").map(str::to_owned))
                    .transpose()?;
                package = Some(Package {
                    name: scalar_text(&form[1], "package name")?,
                    version: scalar_text(&form[2], "package version")?,
                    doc,
                });
            }
            "target" => {
                if form.len() < 2 {
                    bail!("E-PROJECT-001: target requires a name");
                }
                let name = scalar_text(&form[1], "target name")?;
                let attrs = attributes(
                    &form[2..],
                    &["kind", "root", "entry"],
                    &["kind", "root", "entry"],
                    "target",
                )?;
                let kind = symbol(attrs.get("kind").copied(), "target kind")?;
                let root = PathBuf::from(string(attrs["root"], "target root")?);
                let entry = PathBuf::from(string(attrs["entry"], "target entry")?);
                let target = Target { name, root, entry };
                match kind {
                    "lib" => targets.libs.push(target),
                    "bin" => targets.bins.push(target),
                    other => {
                        bail!("E-PROJECT-001: target kind must be `lib` or `bin`, found `{other}`")
                    }
                }
            }
            "dependency" => {
                if form.len() < 2 {
                    bail!("E-PROJECT-001: dependency requires an alias");
                }
                let alias = scalar_text(&form[1], "dependency alias")?;
                let attrs = attributes(
                    &form[2..],
                    &["path", "git", "rev", "wasm"],
                    &[],
                    "dependency",
                )?;
                let path = attrs
                    .get("path")
                    .map(|node| string(node, "dependency path").map(PathBuf::from))
                    .transpose()?;
                let git = attrs
                    .get("git")
                    .map(|node| string(node, "dependency git").map(str::to_owned))
                    .transpose()?;
                let rev = attrs
                    .get("rev")
                    .map(|node| string(node, "dependency revision").map(str::to_owned))
                    .transpose()?;
                let wasm = attrs
                    .get("wasm")
                    .map(|node| string(node, "dependency wasm").map(PathBuf::from))
                    .transpose()?;
                if path.is_none() && git.is_none() {
                    bail!("E-PROJECT-001: dependency requires `path:` or `git:`");
                }
                let dependency = Dependency {
                    path,
                    git,
                    rev,
                    wasm,
                };
                if dependencies.insert(alias.clone(), dependency).is_some() {
                    bail!("E-PROJECT-001: duplicate dependency `{alias}`");
                }
            }
            "plugin-interface" => {
                if form.len() < 2 {
                    bail!("E-PROJECT-001: plugin interface requires a name");
                }
                let name = scalar_text(&form[1], "plugin interface name")?;
                let mut functions = BTreeMap::new();
                for function in forms(&form[2..]) {
                    let function = list(function, "plugin function")?;
                    if function.len() < 2
                        || symbol(function.first(), "plugin function")? != "function"
                    {
                        bail!("E-PROJECT-001: plugin interface children must be `(function ...)`");
                    }
                    let function_name = scalar_text(&function[1], "plugin function name")?;
                    let attrs = attributes(
                        &function[2..],
                        &["params", "result"],
                        &["result"],
                        "plugin function",
                    )?;
                    let params = attrs
                        .get("params")
                        .map(|node| {
                            forms(list(node, "plugin params")?)
                                .map(|node| scalar_text(node, "plugin parameter"))
                                .collect::<Result<Vec<_>>>()
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let result = scalar_text(attrs["result"], "plugin result")?;
                    functions.insert(function_name, PluginFunction { params, result });
                }
                plugin_interfaces.insert(name, PluginInterface { functions });
            }
            other => bail!("E-PROJECT-001: unknown project form `{other}`"),
        }
    }
    Ok(ProjectManifest {
        manifest_version: 1,
        package: package
            .context("E-PROJECT-001: manifest requires `(package <name> <version>)`")?,
        targets,
        dependencies,
        plugin_interfaces,
    })
}

fn forms(nodes: &[crate::syntax::Node]) -> impl Iterator<Item = &crate::syntax::Node> {
    nodes
        .iter()
        .filter(|node| !matches!(node.kind, crate::syntax::NodeKind::Comment(_)))
}

fn list<'a>(node: &'a crate::syntax::Node, context: &str) -> Result<&'a [crate::syntax::Node]> {
    match &node.kind {
        crate::syntax::NodeKind::List(items) => Ok(items),
        _ => bail!("E-PROJECT-001: {context} must be a list"),
    }
}

fn symbol<'a>(node: Option<&'a crate::syntax::Node>, context: &str) -> Result<&'a str> {
    match node.map(|node| &node.kind) {
        Some(crate::syntax::NodeKind::Atom(crate::syntax::Atom::Symbol(value))) => Ok(value),
        _ => bail!("E-PROJECT-001: {context} must start with a symbol"),
    }
}

fn string<'a>(node: &'a crate::syntax::Node, context: &str) -> Result<&'a str> {
    match &node.kind {
        crate::syntax::NodeKind::Atom(crate::syntax::Atom::String(value)) => Ok(value),
        _ => bail!("E-PROJECT-001: {context} must be a string"),
    }
}

fn scalar_text(node: &crate::syntax::Node, context: &str) -> Result<String> {
    match &node.kind {
        crate::syntax::NodeKind::Atom(crate::syntax::Atom::String(value))
        | crate::syntax::NodeKind::Atom(crate::syntax::Atom::Symbol(value)) => Ok(value.clone()),
        _ => bail!("E-PROJECT-001: {context} must be a symbol or string"),
    }
}

fn attributes<'a>(
    nodes: &'a [crate::syntax::Node],
    allowed: &[&str],
    required: &[&str],
    context: &str,
) -> Result<BTreeMap<String, &'a crate::syntax::Node>> {
    let nodes: Vec<_> = forms(nodes).collect();
    if nodes.len() % 2 != 0 {
        bail!("E-PROJECT-001: `{context}` labels must each have exactly one value");
    }
    let mut values = BTreeMap::new();
    for pair in nodes.chunks_exact(2) {
        let label = match &pair[0].kind {
            crate::syntax::NodeKind::Atom(crate::syntax::Atom::Label(label)) => label.as_str(),
            _ => {
                bail!("E-PROJECT-001: `{context}` attributes must be trailing `label: value` pairs")
            }
        };
        if !allowed.contains(&label) {
            bail!("E-PROJECT-001: unknown `{context}` label `{label}:`");
        }
        if values.insert(label.to_owned(), pair[1]).is_some() {
            bail!("E-PROJECT-001: duplicate `{context}` label `{label}:`");
        }
    }
    for label in required {
        if !values.contains_key(*label) {
            bail!("E-PROJECT-001: `{context}` requires `{label}:`");
        }
    }
    Ok(values)
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

pub fn find_project_for_file(path: &Path) -> Result<Option<LoadedProject>> {
    let mut current = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?
        .to_path_buf();
    loop {
        let manifest = current.join(MANIFEST_FILE);
        if manifest.exists() {
            return load_project(&manifest).map(Some);
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

/// Resolve and read every statically declared wasm library for the project
/// containing `entry`. The project-wide check is intentionally repeated here
/// so direct `vibra run` receives the same missing-artifact and signature
/// diagnostics as `vibra check` before lowering or execution.
pub fn static_wasm_artifacts_for_entry(entry: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let Some(project) = find_project_for_file(entry)? else {
        return Ok(BTreeMap::new());
    };
    if !project
        .manifest
        .dependencies
        .values()
        .any(|dependency| dependency.wasm.is_some())
    {
        return Ok(BTreeMap::new());
    }
    check_project(&project.root)?;
    let mut artifacts = BTreeMap::new();
    for alias in sorted_dependency_names(&project.manifest.dependencies) {
        let dependency = &project.manifest.dependencies[alias];
        let Some(relative) = &dependency.wasm else {
            continue;
        };
        let dependency_root = dependency
            .path
            .as_ref()
            .map(|value| resolve_project_path(&project.root, value))
            .unwrap_or_else(|| project.root.join("dep").join(alias));
        let path = dependency_root.join(relative);
        artifacts.insert(
            format!("@{alias}"),
            fs::read(&path)
                .with_context(|| format!("read static wasm artifact {}", path.display()))?,
        );
    }
    Ok(artifacts)
}

pub fn resolve_project_import(project: &LoadedProject, import: &str) -> Result<PathBuf> {
    let namespaces = namespace_roots(project)?;
    resolve_at_import(import, &namespaces)
}

fn resolve_manifest_path(path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_dir() || path.extension().is_none() {
        path.join(MANIFEST_FILE)
    } else {
        path.to_path_buf()
    };
    if !candidate.exists() {
        bail!("project manifest `{}` does not exist", candidate.display());
    }
    if candidate.is_absolute() {
        Ok(candidate)
    } else {
        std::env::current_dir()
            .with_context(|| "resolve current directory")
            .map(|current_dir| current_dir.join(candidate))
    }
}

fn validate_manifest_shape(project: &LoadedProject) -> Result<()> {
    let manifest = &project.manifest;
    if manifest.manifest_version != 1 {
        bail!("manifest-version must be 1");
    }
    validate_name(&manifest.package.name, "package name")?;
    if manifest.targets.libs.is_empty() && manifest.targets.bins.is_empty() {
        bail!("project must declare at least one lib or bin target");
    }

    let mut names = HashSet::new();
    for target in manifest
        .targets
        .libs
        .iter()
        .chain(manifest.targets.bins.iter())
    {
        validate_name(&target.name, "target name")?;
        if !names.insert(target.name.clone()) {
            bail!("duplicate target or dependency name `{}`", target.name);
        }
        validate_relative_source_path(&target.root, "target root")?;
        validate_relative_source_path(&target.entry, "target entry")?;
        let source = project.root.join(&target.root).join(&target.entry);
        if !source.exists() {
            bail!(
                "target `{}` entry `{}` does not exist",
                target.name,
                source.display()
            );
        }
        validate_vibra_extension(&source)?;
    }

    for (name, dep) in &manifest.dependencies {
        validate_name(name, "dependency name")?;
        if !names.insert(name.clone()) {
            bail!("duplicate target or dependency name `{name}`");
        }
        match (&dep.path, &dep.git) {
            (Some(_), Some(_)) => {
                bail!("dependency `{name}` must use either `path` or `git`, not both")
            }
            (None, None) => bail!("dependency `{name}` must declare `path` or `git`"),
            (Some(_), None) => {
                if dep.rev.is_some() {
                    bail!("path dependency `{name}` must not declare `rev`");
                }
            }
            (None, Some(_)) => {
                validate_exact_revision(name, dep.rev.as_deref())?;
            }
        }
        if let Some(wasm) = &dep.wasm {
            validate_relative_artifact_path(wasm, "wasm artifact")
                .with_context(|| format!("dependency `{name}`"))?;
            if wasm.extension().and_then(|value| value.to_str()) != Some("wasm") {
                bail!("dependency `{name}` wasm artifact must end in `.wasm`");
            }
        }
    }
    Ok(())
}

fn validate_relative_artifact_path(path: &Path, label: &str) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("{label} must be a non-empty relative path");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!("{label} must not contain `.` or `..`");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForeignWasmType {
    I32,
    I64,
    F32,
    F64,
}

impl ForeignWasmType {
    /// The `str`/`(array uint8)` buffer types are not themselves scalar wasm
    /// types -- see [`push_foreign_wasm_params_from_type`], which expands
    /// them to a caller-owned `(ptr, len)` pair of `i32`s instead of calling
    /// this directly on them.
    fn from_type_expr(ty: &crate::ast::TypeExpr) -> Option<Self> {
        let crate::ast::TypeExprKind::Named(name) = &ty.value else {
            return None;
        };
        match name.as_str() {
            "bool" | "int8" | "int16" | "int32" | "uint8" | "uint16" | "uint32" => {
                Some(Self::I32)
            }
            "int64" | "uint64" => Some(Self::I64),
            "float32" => Some(Self::F32),
            "float64" => Some(Self::F64),
            _ => None,
        }
    }

    fn from_wasmer(value: wasmer::Type) -> Option<Self> {
        match value {
            wasmer::Type::I32 => Some(Self::I32),
            wasmer::Type::I64 => Some(Self::I64),
            wasmer::Type::F32 => Some(Self::F32),
            wasmer::Type::F64 => Some(Self::F64),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

fn push_foreign_wasm_params_from_type(
    params: &mut Vec<ForeignWasmType>,
    ty: &crate::ast::TypeExpr,
) -> Option<()> {
    if matches!(&ty.value, crate::ast::TypeExprKind::Named(name) if name == "str") {
        params.extend([ForeignWasmType::I32, ForeignWasmType::I32]);
        return Some(());
    }
    if is_uint8_array_type(ty) {
        params.extend([ForeignWasmType::I32, ForeignWasmType::I32]);
        return Some(());
    }
    params.push(ForeignWasmType::from_type_expr(ty)?);
    Some(())
}

fn is_uint8_array_type(ty: &crate::ast::TypeExpr) -> bool {
    let crate::ast::TypeExprKind::Array(inner) = &ty.value else {
        return false;
    };
    matches!(&inner.value, crate::ast::TypeExprKind::Named(name) if name == "uint8")
}

fn is_foreign_buffer_source_type(ty: &crate::ast::TypeExpr) -> bool {
    matches!(&ty.value, crate::ast::TypeExprKind::Named(name) if name == "str") || is_uint8_array_type(ty)
}

fn validate_external_wasm_imports(project: &LoadedProject) -> Result<()> {
    let mut seen = HashSet::new();
    for target in project
        .manifest
        .targets
        .libs
        .iter()
        .chain(project.manifest.targets.bins.iter())
    {
        let entry = project.root.join(&target.root).join(&target.entry);
        validate_module_external_wasm(project, &entry, &mut seen)?;
    }
    Ok(())
}

/// Statically validates external `wasm` FFI wrapper signatures against the
/// declared dependency's actual `.wasm` export, ahead of and independent of
/// the real compile pipeline (`crate::load`/`crate::lower`) -- so it reads
/// `.vibra` S-expression source itself, the same way `crate::frontend` does,
/// rather than reusing a `LoadedProgram`.
fn validate_module_external_wasm(
    project: &LoadedProject,
    path: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    let path = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    if !seen.insert(path.clone()) {
        return Ok(());
    }
    let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let document = crate::syntax::parse(&source)
        .with_context(|| format!("parse S-expression source {}", path.display()))?;
    let document_id = crate::ast::DocumentId::from_path(&path);
    let module = crate::ast::lower_document_with_id(&document, document_id)
        .with_context(|| format!("validate typed source {}", path.display()))?;
    for form in &module.forms {
        let crate::ast::TopLevel::Function(function) = form else {
            continue;
        };
        if let Some((alias, import_name, params, results, requires_memory)) =
            external_wasm_signature(function)?
        {
            validate_external_wasm_export(
                project,
                &function.name.value,
                &alias,
                &import_name,
                &params,
                &results,
                requires_memory,
            )?;
        }
    }
    // Import recursion and path safety are validated by the normal import
    // pass. Reuse its resolved graph so wrappers in imported project modules
    // cannot escape static validation.
    let namespaces = namespace_roots(project)?;
    let parent = path.parent().context("module path has no parent")?;
    for form in &module.forms {
        let crate::ast::TopLevel::Import(import) = form else {
            continue;
        };
        let resolved = if import.path.value.starts_with('@') {
            resolve_at_import(&import.path.value, &namespaces)?
        } else {
            parent.join(&import.path.value)
        };
        validate_module_external_wasm(project, &resolved, seen)?;
    }
    Ok(())
}

fn external_wasm_signature(
    function: &crate::ast::Function,
) -> Result<
    Option<(
        String,
        String,
        Vec<ForeignWasmType>,
        Vec<ForeignWasmType>,
        bool,
    )>,
> {
    let symbol = &function.name.value;
    if function.body.len() != 1 {
        return Ok(None);
    }
    let crate::ast::ExprKind::Wasm { import, .. } = &function.body[0].value else {
        return Ok(None);
    };
    let Some(alias) = import.module.value.strip_prefix('@') else {
        return Ok(None);
    };
    let name = &import.name.value;
    let mut params = Vec::new();
    let mut requires_memory = false;
    for parameter in &function.parameters {
        requires_memory |= is_foreign_buffer_source_type(&parameter.ty);
        push_foreign_wasm_params_from_type(&mut params, &parameter.ty).with_context(|| {
            format!(
                "E-WASM-007: `{symbol}` argument `{}` is not a v1 wasm scalar",
                parameter.name.value
            )
        })?;
    }
    let results = if matches!(&function.return_type.value, crate::ast::TypeExprKind::Named(name) if name == "void")
    {
        Vec::new()
    } else {
        vec![
            ForeignWasmType::from_type_expr(&function.return_type).with_context(|| {
                format!("E-WASM-007: `{symbol}` return type is not a v1 wasm scalar")
            })?,
        ]
    };
    Ok(Some((
        alias.to_string(),
        name.clone(),
        params,
        results,
        requires_memory,
    )))
}

fn validate_external_wasm_export(
    project: &LoadedProject,
    symbol: &str,
    alias: &str,
    name: &str,
    expected_params: &[ForeignWasmType],
    expected_results: &[ForeignWasmType],
    requires_memory: bool,
) -> Result<()> {
    let dependency = project.manifest.dependencies.get(alias).with_context(|| {
        format!("E-WASM-005: `{symbol}` references undeclared wasm dependency `@{alias}`")
    })?;
    let artifact = dependency.wasm.as_ref().with_context(|| {
        format!("E-WASM-005: dependency `{alias}` does not declare a `wasm` artifact")
    })?;
    let dependency_root = dependency
        .path
        .as_ref()
        .map(|value| resolve_project_path(&project.root, value))
        .unwrap_or_else(|| project.root.join("dep").join(alias));
    let artifact = dependency_root.join(artifact);
    if !artifact.is_file() {
        bail!(
            "E-WASM-005: dependency `{alias}` wasm artifact `{}` is missing",
            artifact.display()
        );
    }
    let bytes = fs::read(&artifact).with_context(|| format!("read {}", artifact.display()))?;
    let store = wasmer::Store::default();
    let module = wasmer::Module::new(&store, bytes)
        .with_context(|| format!("E-WASM-005: compile dependency `{alias}` wasm artifact"))?;
    let mut has_memory = false;
    for import in module.imports() {
        let allowed_memory = import.module() == "vibra_ffi"
            && import.name() == "memory"
            && matches!(import.ty(), wasmer::ExternType::Memory(_));
        if !allowed_memory {
            bail!(
                "E-WASM-005: dependency `{alias}` has forbidden wasm import `{}.{}`; v1 permits only `vibra_ffi.memory`",
                import.module(),
                import.name()
            );
        }
        has_memory = true;
    }
    if requires_memory && !has_memory {
        bail!(
            "E-WASM-005: dependency `{alias}` buffer wrapper `{symbol}` requires import `vibra_ffi.memory`"
        );
    }
    let export = module
        .exports()
        .find(|export| export.name() == name)
        .with_context(|| {
            format!("E-WASM-006: dependency `{alias}` does not export function `{name}`")
        })?;
    let wasmer::ExternType::Function(function) = export.ty() else {
        bail!("E-WASM-006: dependency `{alias}` export `{name}` is not a function");
    };
    let actual_params: Option<Vec<_>> = function
        .params()
        .iter()
        .copied()
        .map(ForeignWasmType::from_wasmer)
        .collect();
    let actual_results: Option<Vec<_>> = function
        .results()
        .iter()
        .copied()
        .map(ForeignWasmType::from_wasmer)
        .collect();
    let actual_params = actual_params.with_context(|| {
        format!("E-WASM-007: `{alias}.{name}` uses an unsupported wasm parameter type")
    })?;
    let actual_results = actual_results.with_context(|| {
        format!("E-WASM-007: `{alias}.{name}` uses an unsupported wasm result type")
    })?;
    if actual_params != expected_params || actual_results != expected_results {
        let render = |types: &[ForeignWasmType]| {
            types
                .iter()
                .map(|ty| ty.name())
                .collect::<Vec<_>>()
                .join(", ")
        };
        bail!(
            "E-WASM-007: wrapper `{symbol}` declares ({}) -> ({}), but `@{alias}.{name}` exports ({}) -> ({})",
            render(expected_params), render(expected_results), render(&actual_params), render(&actual_results)
        );
    }
    Ok(())
}

fn validate_dependency_paths(project: &LoadedProject) -> Result<()> {
    for (name, dep) in &project.manifest.dependencies {
        if let Some(path) = &dep.path {
            let resolved = resolve_project_path(&project.root, path);
            if !resolved.exists() {
                bail!(
                    "path dependency `{name}` root `{}` does not exist",
                    resolved.display()
                );
            }
        } else if dep.git.is_some() {
            let resolved = project.root.join("dep").join(name);
            if !resolved.exists() {
                bail!(
                    "git dependency `{name}` is not synced at `{}`",
                    resolved.display()
                );
            }
        }
    }
    Ok(())
}

fn validate_target_imports(
    project: &LoadedProject,
    flags: &crate::load::CompilationFlags,
) -> Result<()> {
    for target in project
        .manifest
        .targets
        .libs
        .iter()
        .chain(project.manifest.targets.bins.iter())
    {
        let entry = project.root.join(&target.root).join(&target.entry);
        crate::load::load_legacy_yaml_program(&entry, flags)
            .map(|_| ())
            .with_context(|| format!("validate imports for target `{}`", target.name))?;
    }
    Ok(())
}

fn namespace_roots(project: &LoadedProject) -> Result<HashMap<String, PathBuf>> {
    let mut roots = HashMap::new();
    for target in project
        .manifest
        .targets
        .libs
        .iter()
        .chain(project.manifest.targets.bins.iter())
    {
        roots.insert(target.name.clone(), project.root.join(&target.root));
    }
    for (name, dep) in &project.manifest.dependencies {
        let root = dep
            .path
            .as_ref()
            .map(|p| resolve_project_path(&project.root, p))
            .unwrap_or_else(|| project.root.join("dep").join(name));
        roots.insert(name.clone(), dependency_library_root(&root, name)?);
    }
    Ok(roots)
}

fn dependency_library_root(root: &Path, dependency_name: &str) -> Result<PathBuf> {
    let manifest_path = root.join(MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(root.to_path_buf());
    }
    let dependency = load_project(&manifest_path)
        .with_context(|| format!("load dependency `{dependency_name}` manifest"))?;
    let libraries = &dependency.manifest.targets.libs;
    let target = libraries
        .iter()
        .find(|target| target.name == dependency_name)
        .or_else(|| (libraries.len() == 1).then(|| &libraries[0]))
        .with_context(|| {
            format!(
                "dependency `{dependency_name}` must expose a matching or single library target"
            )
        })?;
    Ok(root.join(&target.root))
}

fn resolve_at_import(import: &str, namespaces: &HashMap<String, PathBuf>) -> Result<PathBuf> {
    let rest = import
        .strip_prefix('@')
        .context("internal: @ import missing prefix")?;
    let (name, subpath) = rest
        .split_once('/')
        .with_context(|| format!("import `{import}` must use `@name/path`"))?;
    let root = namespaces
        .get(name)
        .with_context(|| format!("unknown @ import namespace `{name}`"))?;
    let subpath = Path::new(subpath);
    validate_relative_source_path(subpath, "@ import path")?;
    Ok(root.join(subpath))
}

fn sorted_dependency_names(dependencies: &HashMap<String, Dependency>) -> Vec<&String> {
    let mut names: Vec<_> = dependencies.keys().collect();
    names.sort();
    names
}

fn validate_exact_revision<'a>(name: &str, rev: Option<&'a str>) -> Result<&'a str> {
    let rev = rev.unwrap_or_default();
    if rev.len() != 40 || !rev.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("E-DEP-001: git dependency `{name}` must pin a full 40-hex commit revision");
    }
    Ok(rev)
}

#[allow(clippy::too_many_arguments)]
fn vendor_git_dependency(
    alias: &str,
    dependency: &Dependency,
    destination: &Path,
    vendor_path: &str,
    packages: &mut Vec<LockedPackage>,
    stack: &mut Vec<String>,
    stdlib_rev: &mut Option<String>,
) -> Result<String> {
    let git = dependency
        .git
        .as_deref()
        .with_context(|| format!("internal: `{alias}` is not a git dependency"))?;
    let rev = validate_exact_revision(alias, dependency.rev.as_deref())?;
    let identity = format!("{git}#{rev}");
    if stack.contains(&identity) {
        bail!("E-DEP-005: dependency cycle reaches `{alias}` at `{identity}`");
    }
    stack.push(identity.clone());

    let clone = tempfile::tempdir().context("create temporary git clone")?;
    let repo = Repository::clone(git, clone.path())
        .with_context(|| format!("clone dependency `{alias}` from {git}"))?;
    let oid = Oid::from_str(rev).context("parse exact dependency revision")?;
    let object = repo
        .find_object(oid, None)
        .with_context(|| format!("revision `{rev}` is unavailable for dependency `{alias}`"))?;
    repo.set_head_detached(oid)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force().remove_ignored(true).remove_untracked(true);
    repo.checkout_tree(&object, Some(&mut checkout))?;

    copy_clean_repository_tree(clone.path(), destination)?;
    let manifest_path = destination.join(MANIFEST_FILE);
    let package_name = if manifest_path.exists() {
        load_project(&manifest_path)?.manifest.package.name
    } else {
        alias.to_string()
    };
    record_stdlib_revision(alias, &package_name, git, rev, stdlib_rev)?;

    let tree_sha256 = hash_package_tree(destination)?;
    let mut edges = BTreeMap::new();
    if manifest_path.exists() {
        let nested = load_project(&manifest_path)?;
        validate_manifest_shape(&nested)?;
        for nested_alias in sorted_dependency_names(&nested.manifest.dependencies) {
            let nested_dep = &nested.manifest.dependencies[nested_alias];
            if nested_dep.path.is_some() {
                bail!(
                    "E-DEP-002: published dependency `{package_name}` uses unsupported path dependency `{nested_alias}`"
                );
            }
            let child_vendor_path = format!("{vendor_path}/dep/{nested_alias}");
            vendor_git_dependency(
                nested_alias,
                nested_dep,
                &destination.join("dep").join(nested_alias),
                &child_vendor_path,
                packages,
                stack,
                stdlib_rev,
            )?;
            edges.insert(nested_alias.clone(), child_vendor_path);
        }
    }

    stack.pop();
    packages.push(LockedPackage {
        name: package_name,
        identity: identity.clone(),
        git: git.to_string(),
        rev: rev.to_ascii_lowercase(),
        tree_sha256,
        vendor_path: vendor_path.to_string(),
        dependencies: edges,
    });
    Ok(identity)
}

fn copy_clean_repository_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries: Vec<_> = fs::read_dir(source)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == ".git" || name == "dep" || name == LOCK_FILE || name == LEGACY_LOCK_FILE {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&name);
        if entry.file_type()?.is_dir() {
            copy_clean_repository_tree(&from, &to)?;
        } else {
            fs::write(&to, canonical_file_bytes(&from)?)
                .with_context(|| format!("export {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

fn record_stdlib_revision(
    alias: &str,
    package_name: &str,
    git: &str,
    rev: &str,
    stdlib_rev: &mut Option<String>,
) -> Result<()> {
    if alias != "std" && package_name != "std" && git != STDLIB_GIT {
        return Ok(());
    }
    if let Some(expected) = stdlib_rev.as_deref() {
        if expected != rev {
            bail!("E-DEP-006: stdlib revision conflict: `{expected}` and `{rev}`");
        }
    } else {
        *stdlib_rev = Some(rev.to_string());
    }
    Ok(())
}

fn hash_package_tree(root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    hash_directory(root, root, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_directory(base: &Path, directory: &Path, hasher: &mut Sha256) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == ".git" || name == "dep" || name == LOCK_FILE || name == LEGACY_LOCK_FILE {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            hash_directory(base, &path, hasher)?;
        } else {
            let relative = path
                .strip_prefix(base)
                .expect("hashed path must be under package root")
                .to_string_lossy()
                .replace('\\', "/");
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(canonical_file_bytes(&path)?);
            hasher.update([0]);
        }
    }
    Ok(())
}

/// Return the canonical bytes used by source vendoring, hashing, and `.vapp`
/// archives. Git checkout settings may rewrite text files to CRLF on Windows;
/// package identities must not depend on that local policy.
pub(crate) fn canonical_file_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)?;
    let text = matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("vibra" | "yaml" | "yml" | "json" | "toml" | "md" | "txt")
    );
    if !text || !bytes.contains(&b'\r') {
        return Ok(bytes);
    }
    let value = std::str::from_utf8(&bytes)
        .with_context(|| format!("text source `{}` is not valid UTF-8", path.display()))?;
    Ok(value.replace("\r\n", "\n").replace('\r', "\n").into_bytes())
}

fn validate_project_lock(project: &LoadedProject) -> Result<()> {
    let has_git_dependencies = project
        .manifest
        .dependencies
        .values()
        .any(|dependency| dependency.git.is_some());
    if !has_git_dependencies {
        return Ok(());
    }
    let lock_path = project.root.join(LOCK_FILE);
    let legacy_lock = project.root.join(LEGACY_LOCK_FILE);
    if !lock_path.exists() && legacy_lock.exists() {
        bail!(
            "E-DEP-003: legacy YAML dependency lock `{}` is unsupported; run `vibra sync`",
            legacy_lock.display()
        );
    }
    let text = fs::read_to_string(&lock_path).with_context(|| {
        format!(
            "E-DEP-003: dependency lock `{}` is missing; run `vibra sync`",
            lock_path.display()
        )
    })?;
    if text.trim_start().starts_with("lock-version:") {
        bail!("E-DEP-003: legacy YAML dependency locks are unsupported; run `vibra sync` to create `{LOCK_FILE}`");
    }
    let lock: ProjectLock = serde_json::from_str(&text)
        .with_context(|| format!("E-DEP-003: parse dependency lock `{}`", lock_path.display()))?;
    if lock.lock_version != 1 {
        bail!("E-DEP-003: dependency lock version must be 1");
    }
    let by_path: HashMap<_, _> = lock
        .packages
        .iter()
        .map(|package| (package.vendor_path.as_str(), package))
        .collect();
    if by_path.len() != lock.packages.len() {
        bail!("E-DEP-003: dependency lock contains duplicate vendor paths");
    }
    let mut visited = HashSet::new();
    let mut stdlib_rev = None;
    for alias in sorted_dependency_names(&project.manifest.dependencies) {
        let dependency = &project.manifest.dependencies[alias];
        if dependency.git.is_some() {
            validate_locked_dependency(
                &project.root,
                alias,
                dependency,
                &format!("dep/{alias}"),
                &by_path,
                &mut visited,
                &mut stdlib_rev,
            )?;
        }
    }
    if visited.len() != lock.packages.len() {
        bail!("E-DEP-003: dependency lock contains stale package entries; run `vibra sync`");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_locked_dependency(
    project_root: &Path,
    alias: &str,
    dependency: &Dependency,
    vendor_path: &str,
    by_path: &HashMap<&str, &LockedPackage>,
    visited: &mut HashSet<String>,
    stdlib_rev: &mut Option<String>,
) -> Result<()> {
    let package = by_path.get(vendor_path).with_context(|| {
        format!("E-DEP-003: dependency lock is missing `{vendor_path}`; run `vibra sync`")
    })?;
    let git = dependency.git.as_deref().unwrap_or_default();
    let rev = validate_exact_revision(alias, dependency.rev.as_deref())?;
    let expected_identity = format!("{git}#{rev}");
    if package.git != git
        || !package.rev.eq_ignore_ascii_case(rev)
        || !package.identity.eq_ignore_ascii_case(&expected_identity)
    {
        bail!("E-DEP-003: dependency lock is stale for `{vendor_path}`; run `vibra sync`");
    }
    let root = project_root.join(Path::new(vendor_path));
    let actual_hash = hash_package_tree(&root).with_context(|| {
        format!("E-DEP-003: vendored dependency `{vendor_path}` is missing or unreadable")
    })?;
    if actual_hash != package.tree_sha256 {
        bail!(
            "E-DEP-004: vendored dependency `{vendor_path}` is dirty: expected {}, found {actual_hash}",
            package.tree_sha256
        );
    }
    record_stdlib_revision(alias, &package.name, git, rev, stdlib_rev)?;
    if !visited.insert(vendor_path.to_string()) {
        bail!("E-DEP-003: duplicate dependency edge for `{vendor_path}`");
    }

    let manifest_path = root.join(MANIFEST_FILE);
    if manifest_path.exists() {
        let nested = load_project(&manifest_path)?;
        if nested.manifest.package.name != package.name {
            bail!(
                "E-DEP-003: dependency lock package name for `{vendor_path}` is stale; run `vibra sync`"
            );
        }
        for nested_alias in sorted_dependency_names(&nested.manifest.dependencies) {
            let nested_dependency = &nested.manifest.dependencies[nested_alias];
            if nested_dependency.path.is_some() {
                bail!(
                    "E-DEP-002: published dependency `{}` uses unsupported path dependency `{nested_alias}`",
                    package.name
                );
            }
            let expected_path = format!("{vendor_path}/dep/{nested_alias}");
            if package.dependencies.get(nested_alias) != Some(&expected_path) {
                bail!(
                    "E-DEP-003: dependency lock edge `{vendor_path}` -> `{nested_alias}` is stale; run `vibra sync`"
                );
            }
            validate_locked_dependency(
                project_root,
                nested_alias,
                nested_dependency,
                &expected_path,
                by_path,
                visited,
                stdlib_rev,
            )?;
        }
        if package.dependencies.len() != nested.manifest.dependencies.len() {
            bail!(
                "E-DEP-003: dependency lock edges for `{vendor_path}` are stale; run `vibra sync`"
            );
        }
    } else if !package.dependencies.is_empty() {
        bail!("E-DEP-003: dependency lock edges for `{vendor_path}` are stale; run `vibra sync`");
    }
    Ok(())
}

pub fn locate_stdlib_source() -> Result<PathBuf> {
    Ok(locate_stdlib_package()?.join("src"))
}

fn locate_stdlib_package() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for ancestor in parent.ancestors() {
                let candidate = ancestor.join("stdlib");
                if candidate.join("src/io.vibra").exists() {
                    let release = ancestor.join("release.json");
                    if release.exists() {
                        validate_distributed_stdlib(&candidate, &release)?;
                    } else if candidate != Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib") {
                        bail!("E-DIST-002: installed toolchain is missing `release.json` beside its bundled stdlib");
                    }
                    return Ok(candidate);
                }
            }
        }
    }

    let candidate = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    if cfg!(debug_assertions) && candidate.join("src/io.vibra").exists() {
        return Ok(candidate);
    }
    bail!("E-DIST-001: cannot locate the bundled stdlib; expected `stdlib/` beside the toolchain `bin/` directory");
}

fn validate_distributed_stdlib(stdlib: &Path, release: &Path) -> Result<()> {
    let text = fs::read_to_string(release)
        .with_context(|| format!("E-DIST-002: read release metadata `{}`", release.display()))?;
    let metadata: ReleaseMetadata = serde_json::from_str(&text)
        .with_context(|| format!("E-DIST-002: parse release metadata `{}`", release.display()))?;
    if metadata.format_version != 1
        || metadata.stdlib_git != STDLIB_GIT
        || metadata.stdlib_rev != STDLIB_REV
    {
        bail!("E-DIST-003: bundled stdlib identity does not match this compiler (expected {STDLIB_GIT}#{STDLIB_REV})");
    }
    let actual = hash_release_stdlib(stdlib)?;
    if actual != metadata.stdlib_sha256 {
        bail!("E-DIST-004: bundled stdlib content is damaged or mismatched: expected {}, found {actual}", metadata.stdlib_sha256);
    }
    Ok(())
}

fn hash_release_stdlib(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    fn collect(root: &Path, directory: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(root, &entry.path(), files)?;
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, entry.path()));
            }
        }
        Ok(())
    }
    collect(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut inventory = String::new();
    for (relative, path) in files {
        inventory.push_str(&format!(
            "{relative} {:x}\n",
            Sha256::digest(fs::read(path)?)
        ));
    }
    Ok(format!("{:x}", Sha256::digest(inventory.as_bytes())))
}

fn write_bin_template(root: &Path, name: &str) -> Result<()> {
    let src = root.join("src").join(name);
    fs::create_dir_all(&src)?;
    fs::write(
        src.join("main.vibra"),
        "(import io \"@std/io.vibra\")\n(fn main () void (do (io.println \"Hello, World!\")))\n",
    )?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(name, &[("bin", name, &format!("src/{name}"), "main.vibra")]),
    )?;
    Ok(())
}

fn write_lib_template(root: &Path, name: &str) -> Result<()> {
    let src = root.join("src").join(name);
    fs::create_dir_all(&src)?;
    fs::write(src.join("lib.vibra"), "(const answer int64 42)\n")?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(name, &[("lib", name, &format!("src/{name}"), "lib.vibra")]),
    )?;
    Ok(())
}

fn write_workspace_template(root: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(root.join("src/core"))?;
    fs::create_dir_all(root.join("src").join(name))?;
    fs::write(
        root.join("src/core/lib.vibra"),
        "(const message str \"Hello from core\")\n",
    )?;
    fs::write(
        root.join("src").join(name).join("main.vibra"),
        "(import io \"@std/io.vibra\")\n(import core \"@core/lib.vibra\")\n(fn main () void (do (io.println core.message)))\n",
    )?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(
            name,
            &[
                ("lib", "core", "src/core", "lib.vibra"),
                ("bin", name, &format!("src/{name}"), "main.vibra"),
            ],
        ),
    )?;
    Ok(())
}

fn manifest_text(name: &str, targets: &[(&str, &str, &str, &str)]) -> String {
    let mut text = format!("(project\n  (package \"{name}\" \"0.1.0\")\n");
    for (kind, target_name, root, entry) in targets {
        text.push_str(&format!(
            "  (target {target_name} kind: {kind} root: \"{root}\" entry: \"{entry}\")\n"
        ));
    }
    text.push_str(&format!(
        "  (dependency std git: \"{STDLIB_GIT}\" rev: \"{STDLIB_REV}\"))\n"
    ));
    text
}

fn write_seeded_stdlib_lock(project_root: &Path) -> Result<()> {
    let vendor_path = "dep/std";
    let package_root = project_root.join(vendor_path);
    let lock = ProjectLock {
        lock_version: 1,
        packages: vec![LockedPackage {
            name: "std".into(),
            identity: format!("{STDLIB_GIT}#{STDLIB_REV}"),
            git: STDLIB_GIT.into(),
            rev: STDLIB_REV.into(),
            tree_sha256: hash_package_tree(&package_root)?,
            vendor_path: vendor_path.into(),
            dependencies: BTreeMap::new(),
        }],
    };
    fs::write(project_root.join(LOCK_FILE), canonical_json(&lock)?)?;
    Ok(())
}

fn normalize_name(name: &str) -> Result<String> {
    let normalized = name.to_ascii_lowercase().replace('_', "-");
    validate_name(&normalized, "project name")?;
    Ok(normalized)
}

fn validate_name(name: &str, context: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("{context} must not be empty");
    };
    if !first.is_ascii_lowercase() {
        bail!("{context} `{name}` must be kebab-case");
    }
    let mut prev_dash = false;
    for ch in chars {
        let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-';
        if !ok || (ch == '-' && prev_dash) {
            bail!("{context} `{name}` must be kebab-case");
        }
        prev_dash = ch == '-';
    }
    if prev_dash {
        bail!("{context} `{name}` must be kebab-case");
    }
    Ok(())
}

fn validate_relative_source_path(path: &Path, context: &str) -> Result<()> {
    if path.is_absolute() {
        bail!("{context} `{}` must be relative", path.display());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => bail!(
                "{context} `{}` must not contain path traversal",
                path.display()
            ),
        }
    }
    Ok(())
}

fn validate_vibra_extension(path: &Path) -> Result<()> {
    let s = path.to_string_lossy();
    if s.ends_with(".vibra") {
        Ok(())
    } else {
        bail!("source `{}` must end in .vibra", path.display());
    }
}

fn resolve_project_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_sources_reject_the_legacy_yaml_extension() {
        validate_vibra_extension(Path::new("src/main.vibra")).unwrap();
        let error = validate_vibra_extension(Path::new("src/main.vibra.yaml")).unwrap_err();
        assert!(error.to_string().contains("must end in .vibra"));
    }

    #[test]
    fn manifest_parser_is_typed_and_rejects_legacy_yaml() {
        let manifest = parse_manifest(
            r#"(project
  (package "sample" "0.1.0" doc: "Sample package.")
  (target core kind: lib root: "src/core" entry: "lib.vibra")
  (target app kind: bin root: "src/app" entry: "main.vibra")
  (dependency local path: "../local")
  (dependency remote git: "https://example.invalid/remote.git" rev: "0123456789abcdef0123456789abcdef01234567" wasm: "remote.wasm")
  (plugin-interface arithmetic
    (function sum params: (int32 int32) result: int32)))"#,
        )
        .unwrap();
        assert_eq!(manifest.package.name, "sample");
        assert_eq!(manifest.package.doc.as_deref(), Some("Sample package."));
        assert_eq!(manifest.targets.libs[0].root, Path::new("src/core"));
        assert_eq!(manifest.targets.bins[0].entry, Path::new("main.vibra"));
        assert_eq!(
            manifest.dependencies["remote"].wasm.as_deref(),
            Some(Path::new("remote.wasm"))
        );
        assert_eq!(
            manifest.plugin_interfaces["arithmetic"].functions["sum"].params,
            ["int32", "int32"]
        );

        let error = parse_manifest("manifest-version: 1\npackage: {}\n").unwrap_err();
        assert!(error.to_string().contains("legacy YAML"));

        for invalid in [
            "(project (package \"x\" \"1\") (target app kind: bin root: \"src\"))",
            "(project (package \"x\" \"1\") (target app kind: bin kind: lib root: \"src\" entry: \"main.vibra\"))",
            "(project (package \"x\" \"1\") (target app kind: bin root: \"src\" entry: \"main.vibra\" extra: true))",
            "(project (package name: \"x\" \"1\") (target app kind: bin root: \"src\" entry: \"main.vibra\"))",
        ] {
            assert!(parse_manifest(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn lock_json_is_sorted_and_newline_terminated() {
        let lock = ProjectLock {
            lock_version: 1,
            packages: vec![LockedPackage {
                name: "sample".into(),
                identity: "git#rev".into(),
                git: "git".into(),
                rev: "rev".into(),
                tree_sha256: "hash".into(),
                vendor_path: "dep/sample".into(),
                dependencies: BTreeMap::from([
                    ("z".into(), "dep/sample/dep/z".into()),
                    ("a".into(), "dep/sample/dep/a".into()),
                ]),
            }],
        };
        let json = canonical_json(&lock).unwrap();
        assert!(json.ends_with('\n'));
        assert!(json.find("\"dependencies\"").unwrap() < json.find("\"git\"").unwrap());
        assert!(json.find("\"a\"").unwrap() < json.find("\"z\"").unwrap());
    }

    #[test]
    fn recursive_vendor_rejects_dependency_cycles_before_fetching() {
        let rev = "0123456789abcdef0123456789abcdef01234567";
        let git = "https://example.test/cycle.git";
        let dependency = Dependency {
            path: None,
            git: Some(git.into()),
            rev: Some(rev.into()),
            wasm: None,
        };
        let temp = tempfile::tempdir().unwrap();
        let mut packages = Vec::new();
        let mut stack = vec![format!("{git}#{rev}")];
        let mut stdlib_rev = None;
        let error = vendor_git_dependency(
            "cycle",
            &dependency,
            &temp.path().join("cycle"),
            "dep/cycle",
            &mut packages,
            &mut stack,
            &mut stdlib_rev,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E-DEP-005"));
    }

    #[test]
    fn dependency_graph_rejects_multiple_stdlib_revisions() {
        let mut selected = None;
        record_stdlib_revision("std", "std", STDLIB_GIT, "aaaa", &mut selected).unwrap();
        let error =
            record_stdlib_revision("std", "std", STDLIB_GIT, "bbbb", &mut selected).unwrap_err();
        assert!(error.to_string().contains("E-DEP-006"));
    }

    #[test]
    fn distributed_stdlib_validates_identity_and_content() {
        let temp = tempfile::tempdir().unwrap();
        let stdlib = temp.path().join("stdlib");
        fs::create_dir(&stdlib).unwrap();
        fs::write(stdlib.join("io.vibra"), "io: true\n").unwrap();
        let digest = hash_release_stdlib(&stdlib).unwrap();
        let release = temp.path().join("release.json");
        fs::write(&release, format!("{{\"format-version\":1,\"stdlib-git\":\"{STDLIB_GIT}\",\"stdlib-rev\":\"{STDLIB_REV}\",\"stdlib-sha256\":\"{digest}\"}}\n")).unwrap();
        validate_distributed_stdlib(&stdlib, &release).unwrap();

        fs::write(stdlib.join("io.vibra"), "tampered: true\n").unwrap();
        assert!(validate_distributed_stdlib(&stdlib, &release)
            .unwrap_err()
            .to_string()
            .contains("E-DIST-004"));
        fs::write(&release, format!("{{\"format-version\":1,\"stdlib-git\":\"{STDLIB_GIT}\",\"stdlib-rev\":\"0000000000000000000000000000000000000000\",\"stdlib-sha256\":\"{digest}\"}}\n")).unwrap();
        assert!(validate_distributed_stdlib(&stdlib, &release)
            .unwrap_err()
            .to_string()
            .contains("E-DIST-003"));
    }

    #[test]
    fn source_hashes_ignore_checkout_line_endings_but_preserve_binary_bytes() {
        let unix = tempfile::tempdir().unwrap();
        let windows = tempfile::tempdir().unwrap();
        fs::write(unix.path().join("module.vibra"), b"value: one\nnext: two\n").unwrap();
        fs::write(
            windows.path().join("module.vibra"),
            b"value: one\r\nnext: two\r\n",
        )
        .unwrap();
        fs::write(unix.path().join("payload.bin"), b"a\r\nb").unwrap();
        fs::write(windows.path().join("payload.bin"), b"a\r\nb").unwrap();

        assert_eq!(
            canonical_file_bytes(&unix.path().join("module.vibra")).unwrap(),
            canonical_file_bytes(&windows.path().join("module.vibra")).unwrap()
        );
        assert_eq!(
            canonical_file_bytes(&windows.path().join("payload.bin")).unwrap(),
            b"a\r\nb"
        );
        assert_eq!(
            hash_package_tree(unix.path()).unwrap(),
            hash_package_tree(windows.path()).unwrap()
        );
    }

    fn wasm_fixture(
        params: Vec<wasm_encoder::ValType>,
        results: Vec<wasm_encoder::ValType>,
    ) -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
            TypeSection,
        };
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function(params, results.clone());
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut exports = ExportSection::new();
        exports.export("sum", ExportKind::Func, 0);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        if results == vec![wasm_encoder::ValType::I32] {
            body.instruction(&Instruction::I32Const(0));
        }
        body.instruction(&Instruction::End);
        code.function(&body);
        module.section(&code);
        module.finish()
    }

    fn ffi_project(wrapper: &str, artifact: Option<Vec<u8>>) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(temp.path().join("foreign")).unwrap();
        fs::write(temp.path().join("src/main.vibra"), wrapper).unwrap();
        if let Some(bytes) = artifact {
            fs::write(temp.path().join("foreign/math.wasm"), bytes).unwrap();
        }
        fs::write(
            temp.path().join(MANIFEST_FILE),
            "(project\n  (package \"ffi-test\" \"0.1.0\")\n  (target ffi-test kind: bin root: \"src\" entry: \"main.vibra\")\n  (dependency math path: \"foreign\" wasm: \"math.wasm\"))\n",
        ).unwrap();
        temp
    }

    const FFI_WRAPPER: &str = "(fn sum ((left int32) (right int32)) int32\n  (do (wasm import: (import \"@math\" \"sum\") args: ((arg left) (arg right)))))\n(fn main () void (do))\n";

    #[test]
    fn check_accepts_matching_static_wasm_export() {
        let project = ffi_project(
            FFI_WRAPPER,
            Some(wasm_fixture(
                vec![wasm_encoder::ValType::I32; 2],
                vec![wasm_encoder::ValType::I32],
            )),
        );
        check_project(project.path()).unwrap();
    }

    #[test]
    fn check_rejects_missing_static_wasm_symbol_before_execution() {
        let project = ffi_project(
            &FFI_WRAPPER.replace("\"sum\")", "\"absent\")"),
            Some(wasm_fixture(
                vec![wasm_encoder::ValType::I32; 2],
                vec![wasm_encoder::ValType::I32],
            )),
        );
        let error = format!("{:#}", check_project(project.path()).unwrap_err());
        assert!(
            error.contains("E-WASM-006") && error.contains("absent"),
            "{error}"
        );
    }

    #[test]
    fn check_rejects_static_wasm_signature_mismatch_before_execution() {
        let project = ffi_project(
            FFI_WRAPPER,
            Some(wasm_fixture(
                vec![wasm_encoder::ValType::I64; 2],
                vec![wasm_encoder::ValType::I32],
            )),
        );
        let error = format!("{:#}", check_project(project.path()).unwrap_err());
        assert!(
            error.contains("E-WASM-007") && error.contains("i64"),
            "{error}"
        );
    }

    #[test]
    fn check_rejects_missing_static_wasm_artifact_before_execution() {
        let project = ffi_project(FFI_WRAPPER, None);
        let error = format!("{:#}", check_project(project.path()).unwrap_err());
        assert!(
            error.contains("E-WASM-005") && error.contains("missing"),
            "{error}"
        );
    }

    #[test]
    fn check_rejects_buffer_wrapper_without_ffi_memory_import() {
        let wrapper = "(fn sum ((text str)) int32\n  (do (wasm import: (import \"@math\" \"sum\") args: ((arg text)))))\n(fn main () void (do))\n";
        let project = ffi_project(
            wrapper,
            Some(wasm_fixture(
                vec![wasm_encoder::ValType::I32; 2],
                vec![wasm_encoder::ValType::I32],
            )),
        );
        let error = format!("{:#}", check_project(project.path()).unwrap_err());
        assert!(
            error.contains("E-WASM-005") && error.contains("vibra_ffi.memory"),
            "{error}"
        );
    }
}
