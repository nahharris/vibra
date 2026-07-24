//! Typed, capability-gated runtime plugin loading.

use crate::project;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginLoadReport {
    report_version: u32,
    interface: String,
    path: String,
    sha256: String,
    functions: Vec<String>,
    instantiated: bool,
}

pub fn load_yaml(
    project_path: &Path,
    interface_name: &str,
    plugin_path: &Path,
    allowed: &[PathBuf],
) -> Result<String> {
    let project = project::load_project(project_path)?;
    let interface = project
        .manifest
        .plugin_interfaces
        .get(interface_name)
        .with_context(|| format!("E-PLUGIN-001: unknown plugin interface `{interface_name}`"))?;
    let plugin_path = std::fs::canonicalize(plugin_path).with_context(|| {
        format!(
            "E-PLUGIN-002: resolve plugin path {}",
            plugin_path.display()
        )
    })?;
    let approved = allowed.iter().any(|scope| {
        std::fs::canonicalize(scope)
            .map(|scope| plugin_path == scope || plugin_path.starts_with(&scope))
            .unwrap_or(false)
    });
    if !approved {
        bail!(
            "E-PLUGIN-003: loading `{}` requires explicit `--allow-plugin-load` authority",
            plugin_path.display()
        );
    }
    let bytes = std::fs::read(&plugin_path)
        .with_context(|| format!("E-PLUGIN-002: read plugin {}", plugin_path.display()))?;
    let mut store = wasmer::Store::default();
    let module = wasmer::Module::new(&store, &bytes)
        .with_context(|| format!("E-PLUGIN-002: compile plugin {}", plugin_path.display()))?;
    if let Some(import) = module.imports().next() {
        bail!(
            "E-PLUGIN-004: plugin imports `{}.{}`; the first milestone grants no ambient authority",
            import.module(),
            import.name()
        );
    }
    let mut functions = Vec::new();
    for (name, expected) in &interface.functions {
        let export = module
            .exports()
            .find(|export| export.name() == name)
            .with_context(|| format!("E-PLUGIN-005: plugin is missing function `{name}`"))?;
        let wasmer::ExternType::Function(actual) = export.ty() else {
            bail!("E-PLUGIN-005: plugin export `{name}` is not a function");
        };
        let expected_params = expected
            .params
            .iter()
            .map(|value| scalar_type(value))
            .collect::<Result<Vec<_>>>()?;
        let expected_results = if expected.result == "void" {
            Vec::new()
        } else {
            vec![scalar_type(&expected.result)?]
        };
        if actual.params() != expected_params || actual.results() != expected_results {
            bail!(
                "E-PLUGIN-006: function `{name}` signature mismatch: expected {:?} -> {:?}, found {:?} -> {:?}",
                expected_params,
                expected_results,
                actual.params(),
                actual.results()
            );
        }
        functions.push(name.clone());
    }
    // Empty imports are the enforcement point for "loading does not mint
    // authority": the plugin receives neither WASI nor Vibra host functions.
    wasmer::Instance::new(&mut store, &module, &wasmer::Imports::new())
        .context("E-PLUGIN-007: instantiate validated plugin")?;
    let report = PluginLoadReport {
        report_version: 1,
        interface: interface_name.into(),
        path: plugin_path.to_string_lossy().replace('\\', "/"),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        functions,
        instantiated: true,
    };
    serde_yaml::to_string(&report).context("serialize plugin load report")
}

fn scalar_type(value: &str) -> Result<wasmer::Type> {
    match value {
        "bool" | "int32" | "uint32" => Ok(wasmer::Type::I32),
        "int64" | "uint64" => Ok(wasmer::Type::I64),
        "float32" => Ok(wasmer::Type::F32),
        "float64" => Ok(wasmer::Type::F64),
        other => bail!("E-PLUGIN-001: unsupported plugin interface type `{other}`"),
    }
}
