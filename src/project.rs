//! Project manifest, scaffold, dependency sync, and import validation.

use anyhow::{bail, Context, Result};
use git2::{build::CheckoutBuilder, Oid, Repository};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MANIFEST_FILE: &str = "project.vibra";
pub const LOCK_FILE: &str = "project.lock.vibra";
pub const STDLIB_GIT: &str = "https://github.com/nahharris/vibra-stdlib.git";
pub const STDLIB_REV: &str = "edc46c6eefb1c0df62b0b5fe4bace2e2f06fec31";

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
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
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
    let lock_text = serde_yaml::to_string(&lock).context("serialize dependency lock")?;
    fs::write(project.root.join(LOCK_FILE), lock_text).context("write dependency lock")?;
    Ok(())
}

pub fn check_project(path: &Path) -> Result<()> {
    let project = load_project(path)?;
    validate_manifest_shape(&project)?;
    validate_dependency_paths(&project)?;
    validate_project_lock(&project)?;
    validate_target_imports(&project)?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<LoadedProject> {
    let manifest_path = resolve_manifest_path(path)?;
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: ProjectManifest = serde_yaml::from_str(&text)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
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

fn validate_target_imports(project: &LoadedProject) -> Result<()> {
    let namespaces = namespace_roots(project)?;
    let mut seen = HashSet::new();
    for target in project
        .manifest
        .targets
        .libs
        .iter()
        .chain(project.manifest.targets.bins.iter())
    {
        let entry = project.root.join(&target.root).join(&target.entry);
        validate_module_imports(&entry, &namespaces, &mut seen)
            .with_context(|| format!("validate imports for target `{}`", target.name))?;
    }
    Ok(())
}

fn validate_module_imports(
    path: &Path,
    namespaces: &HashMap<String, PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    let path = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    if !seen.insert(path.clone()) {
        return Ok(());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let Some(map) = value.as_mapping() else {
        bail!("{}: module root must be a mapping", path.display());
    };
    let parent = path.parent().context("module path has no parent")?;
    for (key, value) in map {
        if key.as_str().is_none() {
            bail!("{}: module keys must be strings", path.display());
        }
        let Some(import) = value
            .as_mapping()
            .and_then(|m| m.get(Value::String("$import".into())))
        else {
            continue;
        };
        let import = import
            .as_str()
            .with_context(|| format!("{}: $import must be a string", path.display()))?;
        let resolved = if import.starts_with('@') {
            resolve_at_import(import, namespaces)?
        } else {
            parent.join(import)
        };
        if !resolved.exists() {
            bail!(
                "{}: import `{import}` resolves to missing `{}`",
                path.display(),
                resolved.display()
            );
        }
        validate_module_imports(&resolved, namespaces, seen)?;
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
        if name == ".git" || name == "dep" || name == LOCK_FILE {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&name);
        if entry.file_type()?.is_dir() {
            copy_clean_repository_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)
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
        if name == ".git" || name == "dep" || name == LOCK_FILE {
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
            hasher.update(fs::read(&path)?);
            hasher.update([0]);
        }
    }
    Ok(())
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
    let text = fs::read_to_string(&lock_path).with_context(|| {
        format!(
            "E-DEP-003: dependency lock `{}` is missing; run `vibra sync`",
            lock_path.display()
        )
    })?;
    let lock: ProjectLock = serde_yaml::from_str(&text)
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
                    return Ok(candidate);
                }
            }
        }
    }

    let candidate = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    if candidate.join("src/io.vibra").exists() {
        return Ok(candidate);
    }
    bail!("cannot locate stdlib directory for project initialization");
}

fn write_bin_template(root: &Path, name: &str) -> Result<()> {
    let src = root.join("src").join(name);
    fs::create_dir_all(&src)?;
    fs::write(
        src.join("main.vibra"),
        "io:\n  $import: \"@std/io.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $io.println: \"Hello, World!\"\n",
    )?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(name, "", &[(name, &format!("src/{name}"), "main.vibra")]),
    )?;
    Ok(())
}

fn write_lib_template(root: &Path, name: &str) -> Result<()> {
    let src = root.join("src").join(name);
    fs::create_dir_all(&src)?;
    fs::write(src.join("lib.vibra"), "answer: 42\n")?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(
            name,
            &format!("    - name: {name}\n      root: src/{name}\n      entry: lib.vibra\n"),
            &[],
        ),
    )?;
    Ok(())
}

fn write_workspace_template(root: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(root.join("src/core"))?;
    fs::create_dir_all(root.join("src").join(name))?;
    fs::write(
        root.join("src/core/lib.vibra"),
        "message: \"Hello from core\"\n",
    )?;
    fs::write(
        root.join("src").join(name).join("main.vibra"),
        "io:\n  $import: \"@std/io.vibra\"\ncore:\n  $import: \"@core/lib.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $io.println: $core.message\n",
    )?;
    fs::write(
        root.join(MANIFEST_FILE),
        manifest_text(
            name,
            "    - name: core\n      root: src/core\n      entry: lib.vibra\n",
            &[(name, &format!("src/{name}"), "main.vibra")],
        ),
    )?;
    Ok(())
}

fn manifest_text(name: &str, libs: &str, bins: &[(&str, &str, &str)]) -> String {
    let mut text =
        format!("manifest-version: 1\npackage:\n  name: {name}\n  version: 0.1.0\n\ntargets:\n");
    if !libs.is_empty() {
        text.push_str("  libs:\n");
        text.push_str(libs);
    }
    if !bins.is_empty() {
        text.push_str("  bins:\n");
        for (bin_name, root, entry) in bins {
            text.push_str(&format!(
                "    - name: {bin_name}\n      root: {root}\n      entry: {entry}\n"
            ));
        }
    }
    text.push_str(&format!(
        "\ndependencies:\n  std:\n    git: {STDLIB_GIT}\n    rev: {STDLIB_REV}\n"
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
    fs::write(project_root.join(LOCK_FILE), serde_yaml::to_string(&lock)?)?;
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
    if s.ends_with(".vibra") || s.ends_with(".vibra.yaml") {
        Ok(())
    } else {
        bail!(
            "source `{}` must end in .vibra or .vibra.yaml",
            path.display()
        );
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
    fn recursive_vendor_rejects_dependency_cycles_before_fetching() {
        let rev = "0123456789abcdef0123456789abcdef01234567";
        let git = "https://example.test/cycle.git";
        let dependency = Dependency {
            path: None,
            git: Some(git.into()),
            rev: Some(rev.into()),
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
}
