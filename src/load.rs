//! Load `.vibra` modules and resolve `$import` relative to each file or project namespace.

use crate::code::SourceDatabase;
use crate::project;
use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_TEMPLATE_SOURCE_BYTES: usize = 1_048_576;
const MAX_TEMPLATE_RENDERED_BYTES: usize = 16_777_216;
const MAX_TEMPLATE_SECTION_DEPTH: usize = 64;

/// Canonical path → parsed YAML root mapping.
#[derive(Debug)]
pub struct LoadedProgram {
    pub entry: PathBuf,
    pub modules: HashMap<PathBuf, Value>,
    pub sources: SourceDatabase,
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

/// Compatibility wrapper for callers that still consume the legacy YAML tree.
#[doc(hidden)]
pub fn load_program(entry: &Path) -> Result<LoadedProgram> {
    load_legacy_yaml_program(entry, &CompilationFlags::default())
}

/// Compatibility wrapper for callers that still consume the legacy YAML tree.
#[doc(hidden)]
pub fn load_program_with_flags(entry: &Path, flags: &CompilationFlags) -> Result<LoadedProgram> {
    load_legacy_yaml_program(entry, flags)
}

/// Temporary compilation-only seam while the staged typed frontend in
/// [`crate::frontend`] is integrated with lowering.
#[doc(hidden)]
pub fn load_legacy_yaml_program(entry: &Path, flags: &CompilationFlags) -> Result<LoadedProgram> {
    flags.validate()?;
    let entry = fs::canonicalize(entry)
        .with_context(|| format!("cannot open entry module {}", entry.display()))?;
    let project = project::find_project_for_file(&entry)?;
    let entry = canonical_module_path(&entry)?;
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    load_recursive(&entry, project.as_ref(), flags, &mut modules, &mut stack)?;
    let (sources, module_parts) = source_database_for_modules(&entry, modules.keys(), None, flags)?;
    let mut modules = rebuild_modules_from_sources(modules.keys(), &sources, &module_parts)?;
    normalize_ignored_annotations(&mut modules)?;
    let embedded_files = expand_embeds(&mut modules)?;
    let modules = crate::macro_expand::expand_program_modules(&entry, modules)?;
    validate_direct_import_aliases(&modules)?;
    Ok(LoadedProgram {
        entry,
        modules,
        sources,
        module_parts,
        embedded_files,
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
    let root = load_module_parts(&entry, &CompilationFlags::new(["test"]))?;
    Ok((entry, root))
}

/// Compatibility wrapper for callers that still consume the legacy YAML tree.
#[doc(hidden)]
pub fn load_inline_program(base_dir: &Path, root: Value) -> Result<LoadedProgram> {
    load_legacy_yaml_inline_program(base_dir, root)
}

/// Temporary YAML-backed `vibra exec` seam pending typed expression lowering.
#[doc(hidden)]
pub fn load_legacy_yaml_inline_program(base_dir: &Path, root: Value) -> Result<LoadedProgram> {
    let base_dir = fs::canonicalize(base_dir)
        .with_context(|| format!("resolve inline base directory {}", base_dir.display()))?;
    let entry = base_dir.join("__vibra_exec__.vibra");
    let project = project::find_project_for_file(&entry)?;
    let mut modules = HashMap::new();
    let mut stack = Vec::new();
    let flags = CompilationFlags::default();
    for import in module_imports(&entry, &root, project.as_ref())? {
        load_recursive(&import, project.as_ref(), &flags, &mut modules, &mut stack)?;
    }
    modules.insert(entry.clone(), root.clone());
    let inline_source = serde_yaml::to_string(&root).context("serialize inline Vibra program")?;
    let (sources, module_parts) = source_database_for_modules(
        &entry,
        modules.keys(),
        Some((&entry, inline_source)),
        &flags,
    )?;
    let mut modules = rebuild_modules_from_sources(modules.keys(), &sources, &module_parts)?;
    normalize_ignored_annotations(&mut modules)?;
    let embedded_files = expand_embeds(&mut modules)?;
    let modules = crate::macro_expand::expand_program_modules(&entry, modules)?;
    validate_direct_import_aliases(&modules)?;
    Ok(LoadedProgram {
        entry,
        modules,
        sources,
        module_parts,
        embedded_files,
    })
}

#[derive(Debug, Clone, Copy)]
enum EmbedFormat {
    Text,
    Binary,
    Yaml,
    Json,
    Toml,
    Xml,
}

fn expand_embeds(modules: &mut HashMap<PathBuf, Value>) -> Result<BTreeMap<PathBuf, String>> {
    let mut embedded = BTreeMap::new();
    let mut paths = modules.keys().cloned().collect::<Vec<_>>();
    paths.sort();
    for module_path in paths {
        let package_root = project::find_project_for_file(&module_path)?
            .map(|project| project.root)
            .unwrap_or_else(|| module_path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let package_root = fs::canonicalize(&package_root).with_context(|| {
            format!(
                "E-EMBED-002: resolve package root {}",
                package_root.display()
            )
        })?;
        let value = modules
            .get_mut(&module_path)
            .expect("module path came from map");
        let mut module_embedded = BTreeMap::new();
        expand_embeds_in_value(value, &module_path, &package_root, &mut module_embedded)?;
        if !module_embedded.is_empty() {
            let mut inventory = String::new();
            for (path, digest) in &module_embedded {
                let relative = path.strip_prefix(&package_root).unwrap_or(path);
                inventory.push_str(&relative.to_string_lossy().replace('\\', "/"));
                inventory.push('\0');
                inventory.push_str(digest);
                inventory.push('\n');
            }
            let fingerprint = format!("{:x}", Sha256::digest(inventory.as_bytes()));
            value
                .as_mapping_mut()
                .context("internal: expanded module root is not a mapping")?
                .insert(
                    Value::String("-embedded-files-sha256".into()),
                    Value::String(fingerprint),
                );
        }
        embedded.extend(module_embedded);
    }
    Ok(embedded)
}

fn expand_embeds_in_value(
    value: &mut Value,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    if let Some(map) = value.as_mapping() {
        if map.len() == 1 {
            if let Some(spec) = map_get_str(map, "$embed") {
                *value = load_embed(spec, module_path, package_root, embedded)?;
                return Ok(());
            }
            if let Some(spec) = map_get_str(map, "$template") {
                *value = load_template(spec, module_path, package_root, embedded)?;
                return Ok(());
            }
        }
    }
    match value {
        Value::Mapping(map) => {
            for (_, child) in map {
                expand_embeds_in_value(child, module_path, package_root, embedded)?;
            }
        }
        Value::Sequence(items) => {
            for child in items {
                expand_embeds_in_value(child, module_path, package_root, embedded)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
enum TemplateNode {
    Text(String),
    Variable(String),
    Section {
        name: String,
        inverted: bool,
        children: Vec<TemplateNode>,
    },
}

fn load_template(
    spec: &Value,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<Value> {
    let options = spec
        .as_mapping()
        .context("E-TEMPLATE-001: `$template` must be a `{path, with}` mapping")?;
    for key in options.keys() {
        let key = key
            .as_str()
            .context("E-TEMPLATE-001: `$template` option keys must be strings")?;
        if !matches!(key, "path" | "with") {
            bail!("E-TEMPLATE-001: unknown `$template` option `{key}`");
        }
    }
    let path = map_get_str(options, "path")
        .and_then(Value::as_str)
        .context("E-TEMPLATE-001: `$template.path` must be a string")?;
    let data = map_get_str(options, "with")
        .and_then(Value::as_mapping)
        .context("E-TEMPLATE-001: `$template.with` must be a mapping")?;
    let resolved = resolve_package_file(
        path,
        module_path,
        package_root,
        "E-TEMPLATE-002",
        "E-TEMPLATE-003",
        "template",
    )?;
    let bytes = fs::read(&resolved)
        .with_context(|| format!("E-TEMPLATE-003: read template {}", resolved.display()))?;
    if bytes.len() > MAX_TEMPLATE_SOURCE_BYTES {
        bail!("E-TEMPLATE-006: template source exceeds {MAX_TEMPLATE_SOURCE_BYTES} bytes");
    }
    let source = String::from_utf8(bytes).context("E-TEMPLATE-003: template is not valid UTF-8")?;
    // Text templates are source inputs, so checkout line endings must not change
    // either their rendered program or the reproducibility fingerprint.
    let source = source.replace("\r\n", "\n");
    embedded.insert(
        resolved.clone(),
        format!("{:x}", Sha256::digest(source.as_bytes())),
    );
    let nodes = parse_template(&source)?;
    let root = Value::Mapping(data.clone());
    let rendered = render_template(&nodes, &[&root])?;
    Ok(literal_string(rendered))
}

fn resolve_package_file(
    path: &str,
    module_path: &Path,
    package_root: &Path,
    path_code: &str,
    read_code: &str,
    kind: &str,
) -> Result<PathBuf> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        bail!("{path_code}: {kind} path must be a normalized relative path: `{path}`");
    }
    let base = module_path
        .parent()
        .with_context(|| format!("{path_code}: module has no parent"))?;
    let resolved = fs::canonicalize(base.join(relative)).with_context(|| {
        format!(
            "{read_code}: cannot read {kind} file `{path}` from {}",
            module_path.display()
        )
    })?;
    if !resolved.starts_with(package_root) || !resolved.is_file() {
        bail!(
            "{path_code}: {kind} file `{}` escapes package root {}",
            resolved.display(),
            package_root.display()
        );
    }
    Ok(resolved)
}

fn parse_template(source: &str) -> Result<Vec<TemplateNode>> {
    let (nodes, offset) = parse_template_nodes(source, 0, None, 0)?;
    debug_assert_eq!(offset, source.len());
    Ok(nodes)
}

fn parse_template_nodes(
    source: &str,
    mut offset: usize,
    closing: Option<&str>,
    depth: usize,
) -> Result<(Vec<TemplateNode>, usize)> {
    if depth > MAX_TEMPLATE_SECTION_DEPTH {
        bail!("E-TEMPLATE-006: template section nesting exceeds {MAX_TEMPLATE_SECTION_DEPTH}");
    }
    let mut nodes = Vec::new();
    while offset < source.len() {
        let Some(relative_open) = source[offset..].find("{{") else {
            nodes.push(TemplateNode::Text(source[offset..].to_string()));
            offset = source.len();
            break;
        };
        let open = offset + relative_open;
        if open > offset {
            nodes.push(TemplateNode::Text(source[offset..open].to_string()));
        }
        if source[open..].starts_with("{{{") {
            let content_start = open + 3;
            let close = source[content_start..]
                .find("}}}")
                .map(|relative| content_start + relative)
                .context("E-TEMPLATE-004: unclosed triple-brace variable")?;
            let name = template_name(&source[content_start..close], "variable")?;
            nodes.push(TemplateNode::Variable(name));
            offset = close + 3;
            continue;
        }
        let content_start = open + 2;
        let close = source[content_start..]
            .find("}}")
            .map(|relative| content_start + relative)
            .context("E-TEMPLATE-004: unclosed template tag")?;
        let tag = source[content_start..close].trim();
        offset = close + 2;
        if let Some(name) = tag.strip_prefix('/') {
            let name = template_name(name, "closing section")?;
            if closing == Some(name.as_str()) {
                return Ok((nodes, offset));
            }
            bail!("E-TEMPLATE-004: unexpected closing section `{name}`");
        }
        if let Some(raw_name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let name = template_name(raw_name, "section")?;
            let (children, next) = parse_template_nodes(source, offset, Some(&name), depth + 1)?;
            nodes.push(TemplateNode::Section {
                name,
                inverted: tag.starts_with('^'),
                children,
            });
            offset = next;
            continue;
        }
        if tag.starts_with('!') {
            continue;
        }
        let name = template_name(tag.strip_prefix('&').unwrap_or(tag), "variable")?;
        nodes.push(TemplateNode::Variable(name));
    }
    if let Some(name) = closing {
        bail!("E-TEMPLATE-004: unclosed section `{name}`");
    }
    Ok((nodes, offset))
}

fn template_name(raw: &str, kind: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty()
        || (name != "."
            && name
                .split('.')
                .any(|segment| segment.is_empty() || !is_template_identifier(segment)))
    {
        bail!("E-TEMPLATE-004: invalid {kind} name `{name}`");
    }
    Ok(name.to_string())
}

fn is_template_identifier(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn render_template(nodes: &[TemplateNode], contexts: &[&Value]) -> Result<String> {
    let mut output = String::new();
    render_template_into(nodes, contexts, &mut output)?;
    Ok(output)
}

fn render_template_into(
    nodes: &[TemplateNode],
    contexts: &[&Value],
    output: &mut String,
) -> Result<()> {
    for node in nodes {
        match node {
            TemplateNode::Text(text) => append_template_output(output, text)?,
            TemplateNode::Variable(name) => {
                let value = lookup_template_value(contexts, name)
                    .with_context(|| format!("E-TEMPLATE-005: unknown template value `{name}`"))?;
                append_template_output(output, &render_template_scalar(name, value)?)?;
            }
            TemplateNode::Section {
                name,
                inverted,
                children,
            } => {
                let value = lookup_template_value(contexts, name).with_context(|| {
                    format!("E-TEMPLATE-005: unknown template section `{name}`")
                })?;
                let truthy = template_truthy(value);
                if *inverted {
                    if !truthy {
                        render_template_into(children, contexts, output)?;
                    }
                } else if truthy {
                    match value {
                        Value::Sequence(items) => {
                            for item in items {
                                let mut nested = contexts.to_vec();
                                nested.push(item);
                                render_template_into(children, &nested, output)?;
                            }
                        }
                        _ => {
                            let mut nested = contexts.to_vec();
                            nested.push(value);
                            render_template_into(children, &nested, output)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn append_template_output(output: &mut String, value: &str) -> Result<()> {
    let length = output
        .len()
        .checked_add(value.len())
        .context("E-TEMPLATE-006: rendered template size overflow")?;
    if length > MAX_TEMPLATE_RENDERED_BYTES {
        bail!("E-TEMPLATE-006: rendered template exceeds {MAX_TEMPLATE_RENDERED_BYTES} bytes");
    }
    output.push_str(value);
    Ok(())
}

fn lookup_template_value<'a>(contexts: &[&'a Value], name: &str) -> Option<&'a Value> {
    if name == "." {
        return contexts.last().copied();
    }
    let mut segments = name.split('.');
    let first = segments.next()?;
    for context in contexts.iter().rev() {
        let Some(mut value) = context
            .as_mapping()
            .and_then(|mapping| mapping.get(Value::String(first.to_string())))
        else {
            continue;
        };
        for segment in segments.clone() {
            value = value
                .as_mapping()?
                .get(Value::String(segment.to_string()))?;
        }
        return Some(value);
    }
    None
}

fn render_template_scalar(name: &str, value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.clone()),
        _ => bail!("E-TEMPLATE-005: template value `{name}` must be a scalar"),
    }
}

fn template_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Sequence(items) => !items.is_empty(),
        _ => true,
    }
}

fn load_embed(
    spec: &Value,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<Value> {
    let (path, requested_format) = if let Some(path) = spec.as_str() {
        (path, "auto")
    } else {
        let map = spec
            .as_mapping()
            .context("E-EMBED-001: `$embed` must be a path string or `{path, format}` mapping")?;
        for key in map.keys() {
            let key = key
                .as_str()
                .context("E-EMBED-001: `$embed` option keys must be strings")?;
            if !matches!(key, "path" | "format") {
                bail!("E-EMBED-001: unknown `$embed` option `{key}`");
            }
        }
        let path = map_get_str(map, "path")
            .and_then(Value::as_str)
            .context("E-EMBED-001: `$embed.path` must be a string")?;
        let format = map_get_str(map, "format")
            .map(|v| {
                v.as_str()
                    .context("E-EMBED-001: `$embed.format` must be a string")
            })
            .transpose()?
            .unwrap_or("auto");
        (path, format)
    };
    let resolved = resolve_package_file(
        path,
        module_path,
        package_root,
        "E-EMBED-002",
        "E-EMBED-003",
        "embedded",
    )?;
    let bytes = fs::read(&resolved)
        .with_context(|| format!("E-EMBED-003: read embedded file {}", resolved.display()))?;
    embedded.insert(resolved.clone(), format!("{:x}", Sha256::digest(&bytes)));
    let format = parse_embed_format(requested_format, &resolved)?;
    match format {
        EmbedFormat::Text => Ok(literal_string(
            String::from_utf8(bytes).context("E-EMBED-004: text embed is not valid UTF-8")?,
        )),
        EmbedFormat::Binary => Ok(binary_expr(&bytes)),
        EmbedFormat::Yaml => {
            let parsed: Value =
                serde_yaml::from_slice(&bytes).context("E-EMBED-004: parse embedded YAML")?;
            structured_expr(parsed)
        }
        EmbedFormat::Json => {
            let parsed: serde_json::Value =
                serde_json::from_slice(&bytes).context("E-EMBED-004: parse embedded JSON")?;
            structured_expr(serde_yaml::to_value(parsed)?)
        }
        EmbedFormat::Toml => {
            let text = std::str::from_utf8(&bytes)
                .context("E-EMBED-004: TOML embed is not valid UTF-8")?;
            let parsed: toml::Value =
                toml::from_str(text).context("E-EMBED-004: parse embedded TOML")?;
            structured_expr(serde_yaml::to_value(parsed)?)
        }
        EmbedFormat::Xml => {
            let text =
                std::str::from_utf8(&bytes).context("E-EMBED-004: XML embed is not valid UTF-8")?;
            let parsed: serde_json::Value =
                quick_xml::de::from_str(text).context("E-EMBED-004: parse embedded XML")?;
            structured_expr(serde_yaml::to_value(parsed)?)
        }
    }
}

fn parse_embed_format(requested: &str, path: &Path) -> Result<EmbedFormat> {
    let name = if requested == "auto" {
        path.extension().and_then(|v| v.to_str()).unwrap_or("")
    } else {
        requested
    };
    match name.to_ascii_lowercase().as_str() {
        "text" | "txt" => Ok(EmbedFormat::Text),
        "binary" | "bin" => Ok(EmbedFormat::Binary),
        "yaml" | "yml" => Ok(EmbedFormat::Yaml),
        "json" => Ok(EmbedFormat::Json),
        "toml" => Ok(EmbedFormat::Toml),
        "xml" => Ok(EmbedFormat::Xml),
        _ => bail!(
            "E-EMBED-001: cannot infer embed format for {}; use text, binary, yaml, json, toml, or xml",
            path.display()
        ),
    }
}

fn literal_string(value: String) -> Value {
    let mut map = Mapping::new();
    map.insert(Value::String("$literal".into()), Value::String(value));
    Value::Mapping(map)
}

fn binary_expr(bytes: &[u8]) -> Value {
    let items = bytes
        .iter()
        .map(|byte| {
            let mut typed = Mapping::new();
            typed.insert(Value::String("value".into()), Value::Number((*byte).into()));
            typed.insert(Value::String("type".into()), Value::String("$uint8".into()));
            let mut literal = Mapping::new();
            literal.insert(Value::String("$literal".into()), Value::Mapping(typed));
            Value::Mapping(literal)
        })
        .collect();
    let mut array = Mapping::new();
    array.insert(Value::String("$array".into()), Value::Sequence(items));
    Value::Mapping(array)
}

fn structured_expr(value: Value) -> Result<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(value),
        Value::String(value) => Ok(literal_string(value)),
        Value::Sequence(items) => {
            let items = items
                .into_iter()
                .map(structured_expr)
                .collect::<Result<_>>()?;
            let mut array = Mapping::new();
            array.insert(Value::String("$array".into()), Value::Sequence(items));
            Ok(Value::Mapping(array))
        }
        Value::Mapping(items) => {
            let mut fields = Mapping::new();
            for (key, value) in items {
                let key = key
                    .as_str()
                    .context("E-EMBED-005: structured mapping keys must be strings")?;
                fields.insert(Value::String(key.into()), structured_expr(value)?);
            }
            let mut record = Mapping::new();
            record.insert(Value::String("$record".into()), Value::Mapping(fields));
            Ok(Value::Mapping(record))
        }
        Value::Tagged(_) => bail!("E-EMBED-005: YAML tags are not supported in embedded data"),
    }
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
    flags: &CompilationFlags,
) -> Result<(SourceDatabase, HashMap<PathBuf, Vec<PathBuf>>)> {
    let mut source_pairs = Vec::new();
    let mut module_parts = HashMap::new();
    for module in modules {
        if inline.as_ref().is_some_and(|(path, _)| *path == module) {
            continue;
        }
        let parts = module_part_paths(module, flags)?;
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
    flags: &CompilationFlags,
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

    let v = load_module_parts(&path, flags)?;
    let imports = module_imports(&path, &v, project)?;

    for imp in imports {
        load_recursive(&imp, project, flags, modules, stack)?;
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

fn load_module_parts(module_path: &Path, flags: &CompilationFlags) -> Result<Value> {
    let mut merged = Mapping::new();
    for part in module_part_paths(module_path, flags)? {
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

fn module_part_paths(module_path: &Path, flags: &CompilationFlags) -> Result<Vec<PathBuf>> {
    let mut paths = vec![module_path.to_path_buf()];
    let Some(parent) = module_path.parent() else {
        return Ok(paths);
    };
    let Some(file_name) = module_path.file_name().and_then(|s| s.to_str()) else {
        return Ok(paths);
    };
    let Some(base) = vibra_file_stem(file_name) else {
        return Ok(paths);
    };
    let prefix = format!("{base}.");
    for entry in fs::read_dir(parent).with_context(|| format!("read {}", parent.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path == module_path {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = vibra_file_stem(file_name) else {
            continue;
        };
        let Some(suffix) = stem.strip_prefix(&prefix) else {
            continue;
        };
        let suffix_flags = suffix.split('.').collect::<Vec<_>>();
        if suffix.is_empty() || suffix_flags.iter().any(|flag| !is_compilation_flag(flag)) {
            bail!(
                "E-FLAG-002: malformed conditional source suffix in `{file_name}`; expected one or more dot-separated kebab-case flags"
            );
        }
        if suffix_flags.iter().all(|flag| flags.contains(flag)) {
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
    for candidate_name in [format!("{base}.vibra"), format!("{base}.vibra.yaml")] {
        let candidate = path.with_file_name(candidate_name);
        if candidate.exists() {
            return fs::canonicalize(candidate)
                .with_context(|| format!("resolve base module for {file_name}"));
        }
    }
    Ok(path)
}

fn is_vibra_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".vibra") || s.ends_with(".vibra.yaml")
}

fn vibra_file_stem(file_name: &str) -> Option<&str> {
    file_name
        .strip_suffix(".vibra.yaml")
        .or_else(|| file_name.strip_suffix(".vibra"))
}

fn key_as_str(k: &Value) -> Result<&str> {
    k.as_str()
        .ok_or_else(|| anyhow::anyhow!("mapping key must be a string"))
}

pub fn map_get_str<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}
