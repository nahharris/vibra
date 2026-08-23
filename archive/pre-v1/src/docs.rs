//! Source-level documentation discovery for CLI and editor tooling.

use crate::{frontend, load, project, typed_readers};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
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
    let program = frontend::load_surface_program(entry, flags)?;
    let project = project::find_project_for_file(&program.entry)?;
    let mut docs = typed_readers::staged_collect_typed_docs(&program)?
        .into_iter()
        .map(|documentation| {
            (
                documentation.symbol.clone(),
                Documentation {
                    symbol: documentation.symbol,
                    kind: documentation.kind,
                    documentation: documentation.documentation,
                    module: documentation.module,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

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
