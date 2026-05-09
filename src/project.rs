//! Project manifest, scaffold, dependency sync, and import validation.

use anyhow::{bail, Context, Result};
use git2::{Oid, Repository};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MANIFEST_FILE: &str = "project.vibra";

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
    copy_dir_recursive(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib"),
        &path.join("dep/std"),
    )
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
    Ok(())
}

pub fn sync_project(path: &Path) -> Result<()> {
    let project = load_project(path)?;
    validate_manifest_shape(&project)?;
    for (name, dep) in &project.manifest.dependencies {
        if let Some(git) = &dep.git {
            let rev = dep
                .rev
                .as_deref()
                .with_context(|| format!("git dependency `{name}` must pin `rev`"))?;
            let dest = project.root.join("dep").join(name);
            sync_git_dependency(git, rev, &dest)
                .with_context(|| format!("sync git dependency `{name}`"))?;
        }
    }
    Ok(())
}

pub fn check_project(path: &Path) -> Result<()> {
    let project = load_project(path)?;
    validate_manifest_shape(&project)?;
    validate_dependency_paths(&project)?;
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
    let namespaces = namespace_roots(project);
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
    Ok(candidate)
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
                if dep.rev.as_deref().is_none_or(str::is_empty) {
                    bail!("git dependency `{name}` must pin `rev`");
                }
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
    let namespaces = namespace_roots(project);
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
        let Some(key) = key.as_str() else {
            bail!("{}: module keys must be strings", path.display());
        };
        if key.starts_with('-') {
            continue;
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

fn namespace_roots(project: &LoadedProject) -> HashMap<String, PathBuf> {
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
        roots.insert(name.clone(), root);
    }
    roots
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

fn sync_git_dependency(url: &str, rev: &str, dest: &Path) -> Result<()> {
    let repo = if dest.exists() {
        let repo = Repository::open(dest).with_context(|| format!("open {}", dest.display()))?;
        {
            let mut remote = repo
                .find_remote("origin")
                .or_else(|_| repo.remote_anonymous(url))?;
            remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
        }
        repo
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        Repository::clone(url, dest)
            .with_context(|| format!("clone {url} into {}", dest.display()))?
    };
    let oid = Oid::from_str(rev).with_context(|| format!("invalid git rev `{rev}`"))?;
    let object = repo.find_object(oid, None)?;
    repo.checkout_tree(&object, None)?;
    repo.set_head_detached(oid)?;
    Ok(())
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
    text.push_str("\ndependencies:\n  std:\n    path: dep/std\n");
    text
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("copy {} to {}", src_path.display(), dst_path.display())
            })?;
        }
    }
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
