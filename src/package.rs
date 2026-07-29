//! Deterministic `.vapp` application archives.

use crate::{frontend, load, project, runtime::RunConfig, typed_program, wasm_backend};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const METADATA: &str = "package.json";
const LEGACY_METADATA: &str = "package.vibra";
const WASM: &str = "program.wasm";
const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackageMetadata {
    format_version: u32,
    name: String,
    version: String,
    target: String,
    runtime_abi: String,
    compiler_version: String,
    stdlib_git: String,
    stdlib_rev: String,
    entry: String,
    #[serde(default)]
    compilation_flags: Vec<String>,
    #[serde(default)]
    selected_sources: Vec<String>,
    files: BTreeMap<String, String>,
}

pub fn build(project_path: &Path, target_name: Option<&str>, output: &Path) -> Result<()> {
    build_with_flags(
        project_path,
        target_name,
        output,
        &load::CompilationFlags::default(),
    )
}

pub fn build_with_flags(
    project_path: &Path,
    target_name: Option<&str>,
    output: &Path,
    flags: &load::CompilationFlags,
) -> Result<()> {
    project::check_project_with_flags(project_path, flags)
        .context("E-PKG-001: project is not buildable")?;
    let loaded = project::load_project(project_path)?;
    let target = match target_name {
        Some(name) => loaded
            .manifest
            .targets
            .bins
            .iter()
            .find(|target| target.name == name)
            .with_context(|| format!("E-PKG-002: bin target `{name}` does not exist"))?,
        None if loaded.manifest.targets.bins.len() == 1 => &loaded.manifest.targets.bins[0],
        None if loaded.manifest.targets.bins.is_empty() => {
            bail!("E-PKG-002: project has no bin target")
        }
        None => bail!("E-PKG-002: project has multiple bin targets; select one with --bin"),
    };
    let entry_path = loaded.root.join(&target.root).join(&target.entry);
    let program = frontend::load_surface_program(&entry_path, flags)
        .context("E-PKG-003: load application")?;
    let lowered =
        typed_program::lower_typed_program(&program).context("E-PKG-003: lower application")?;
    let wasm = wasm_backend::emit_program_wasm(&lowered);

    let mut payloads = BTreeMap::<String, Vec<u8>>::new();
    collect_project_files(&loaded.root, &loaded.root, output, &mut payloads)?;
    payloads.insert(WASM.into(), wasm);
    let files = payloads
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256(bytes)))
        .collect();
    let entry = format!("source/{}/{}", slash(&target.root), slash(&target.entry));
    let mut selected_sources = program
        .module_parts
        .values()
        .flatten()
        .map(|path| package_source_name(&loaded.root, path))
        .collect::<Vec<_>>();
    selected_sources.sort();
    selected_sources.dedup();
    let metadata = PackageMetadata {
        format_version: FORMAT_VERSION,
        name: loaded.manifest.package.name,
        version: loaded.manifest.package.version,
        target: target.name.clone(),
        runtime_abi: "vibra-v1".into(),
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        stdlib_git: project::STDLIB_GIT.into(),
        stdlib_rev: project::STDLIB_REV.into(),
        entry,
        compilation_flags: flags.iter().map(str::to_string).collect(),
        selected_sources,
        files,
    };
    let metadata_bytes = canonical_json(&metadata)?;
    let file =
        File::create(output).with_context(|| format!("E-PKG-004: create {}", output.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    zip.start_file(METADATA, options)?;
    zip.write_all(&metadata_bytes)?;
    for (name, bytes) in payloads {
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    Ok(())
}

pub fn inspect(path: &Path) -> Result<String> {
    let (_, metadata) = open_verified(path)?;
    String::from_utf8(canonical_json(&metadata)?).context("serialize package metadata")
}

pub fn verify(path: &Path) -> Result<()> {
    open_verified(path).map(|_| ())
}

pub fn run(path: &Path, config: &RunConfig) -> Result<()> {
    let (mut archive, metadata) = open_verified(path)?;
    let temp = tempfile::tempdir().context("E-PKG-010: create package workspace")?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_owned();
        let Some(relative) = name.strip_prefix("source/") else {
            continue;
        };
        validate_archive_path(relative)?;
        let destination = temp.path().join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        fs::write(destination, bytes)?;
    }
    let entry = metadata
        .entry
        .strip_prefix("source/")
        .context("E-PKG-006: invalid entry path")?;
    validate_archive_path(entry)?;
    let flags = load::CompilationFlags::try_new(metadata.compilation_flags.iter().cloned())
        .context("E-PKG-006: invalid compilation flags")?;
    let program = frontend::load_surface_program(&temp.path().join(entry), &flags)
        .context("E-PKG-011: load packaged application")?;
    let lowered = typed_program::lower_typed_program(&program)
        .context("E-PKG-011: lower packaged application")?;
    let expected = read_entry(&mut archive, WASM)?;
    if wasm_backend::emit_program_wasm(&lowered) != expected {
        bail!("E-PKG-008: packaged Wasm does not match packaged sources");
    }
    wasm_backend::run_wasm(&expected, config)
}

fn package_source_name(root: &Path, path: &Path) -> String {
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&canonical_root)
        .map(|relative| format!("source/{}", slash(relative)))
        .unwrap_or_else(|_| format!("external:{}", slash(path)))
}

fn open_verified(path: &Path) -> Result<(ZipArchive<File>, PackageMetadata)> {
    let file = File::open(path).with_context(|| format!("E-PKG-005: open {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("E-PKG-005: invalid .vapp ZIP")?;
    if archive.by_name(LEGACY_METADATA).is_ok() {
        bail!(
            "E-PKG-007: package format 1 metadata `{LEGACY_METADATA}` is no longer supported; rebuild the .vapp"
        );
    }
    let metadata_bytes = read_entry(&mut archive, METADATA)?;
    let metadata: PackageMetadata = serde_json::from_slice(&metadata_bytes)
        .context("E-PKG-006: invalid package JSON metadata")?;
    if canonical_json(&metadata)? != metadata_bytes {
        bail!("E-PKG-006: package metadata is not canonical JSON");
    }
    if metadata.format_version != FORMAT_VERSION || metadata.runtime_abi != "vibra-v1" {
        bail!("E-PKG-007: incompatible package format or runtime ABI");
    }
    let mut actual = BTreeMap::<String, usize>::new();
    for index in 0..archive.len() {
        let name = archive.by_index(index)?.name().to_owned();
        validate_archive_path(&name)?;
        *actual.entry(name).or_default() += 1;
    }
    for (name, count) in &actual {
        if *count != 1 {
            bail!("E-PKG-006: duplicate archive entry `{name}`");
        }
        if name != METADATA && !metadata.files.contains_key(name) {
            bail!("E-PKG-006: undeclared archive entry `{name}`");
        }
    }
    if actual.len() != metadata.files.len() + 1 {
        bail!("E-PKG-006: package inventory is incomplete");
    }
    for (name, expected) in &metadata.files {
        validate_archive_path(name)?;
        let bytes = read_entry(&mut archive, name)?;
        if sha256(&bytes) != *expected {
            bail!("E-PKG-009: checksum mismatch for `{name}`");
        }
    }
    Ok((archive, metadata))
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    validate_archive_path(name)?;
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("E-PKG-006: missing `{name}`"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("E-PKG-009: cannot read `{name}`"))?;
    Ok(bytes)
}

fn collect_project_files(
    root: &Path,
    directory: &Path,
    output: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let first = relative.components().next().and_then(|c| match c {
            Component::Normal(v) => v.to_str(),
            _ => None,
        });
        if matches!(first, Some(".git" | ".worktrees" | "target"))
            || path == output
            || path.extension().and_then(|v| v.to_str()) == Some("vapp")
        {
            continue;
        }
        if path.is_dir() {
            collect_project_files(root, &path, output, files)?;
        } else {
            files.insert(
                format!("source/{}", slash(relative)),
                project::canonical_file_bytes(&path)?,
            );
        }
    }
    Ok(())
}

fn validate_archive_path(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    if path.contains('\\')
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("E-PKG-006: unsafe archive path `{path}`");
    }
    Ok(())
}

fn slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).context("serialize canonical package JSON")?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_yaml_metadata_is_rejected_as_format_one() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("legacy.vapp");
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file(LEGACY_METADATA, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"format-version: 1\n").unwrap();
        zip.finish().unwrap();

        let error = verify(&path).unwrap_err().to_string();
        assert!(error.contains("E-PKG-007"), "{error}");
        assert!(error.contains("rebuild"), "{error}");
    }

    #[test]
    fn canonical_json_is_compact_and_lf_terminated() {
        let value = BTreeMap::from([("a", 1), ("b", 2)]);
        assert_eq!(canonical_json(&value).unwrap(), b"{\"a\":1,\"b\":2}\n");
    }

    #[test]
    fn package_build_and_validation_do_not_use_the_legacy_value_loader() {
        assert!(
            !include_str!("package.rs").contains("load_legacy_yaml_program"),
            "package source validation must lower directly from the typed frontend"
        );
    }
}
