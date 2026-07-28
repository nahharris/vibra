//! Staged typed S-expression frontend foundation.
//!
//! This module owns physical source selection, parsing, typed surface
//! validation, deterministic module-part merging, import discovery, and typed
//! macro expansion. Post-expansion symbol/import validation runs before the
//! remaining compile-time phases. This module deliberately has no conversion
//! to the legacy YAML value tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::ast::{
    self, AnnotationKind, DocumentId, EmbedFormat, Expr, ExprField, ExprKind, ImplItem, Literal,
    MethodBinding, Module, Origin, Pattern, PatternKind, Spanned, TopLevel, TypeExpr, TypeExprKind,
};
use crate::load::{is_compilation_flag, CompilationFlags};
use crate::project_context::{self, ProjectImportContext};
use crate::syntax::{self, Document};

#[derive(Debug, Clone)]
pub struct SourcePart {
    pub path: PathBuf,
    pub source: String,
    pub document: Document,
    pub module: Module,
}

#[derive(Debug, Clone)]
pub struct SourceModule {
    pub path: PathBuf,
    pub parts: Vec<SourcePart>,
}

impl SourceModule {
    /// Iterate the logical module surface without erasing each physical
    /// document's identity or source-relative spans.
    pub fn forms(&self) -> impl Iterator<Item = &TopLevel> {
        self.parts.iter().flat_map(|part| part.module.forms.iter())
    }
}

#[derive(Debug)]
pub struct SurfaceProgram {
    pub entry: PathBuf,
    pub modules: BTreeMap<PathBuf, SourceModule>,
    pub module_parts: BTreeMap<PathBuf, Vec<PathBuf>>,
    /// Canonical compile-time input path to its raw-content SHA-256.
    pub embedded_files: BTreeMap<PathBuf, String>,
    /// Resolved import alias -> target module path, keyed by module path.
    /// Reused by editor tooling (e.g. the S-expression semantic index) so
    /// import resolution has exactly one implementation.
    pub imports: BTreeMap<PathBuf, BTreeMap<String, PathBuf>>,
}

const MAX_TEMPLATE_SOURCE_BYTES: usize = 1_048_576;
const MAX_EMBED_SOURCE_BYTES: usize = 1_048_576;
const MAX_TEMPLATE_RENDERED_BYTES: usize = 16_777_216;
const MAX_TEMPLATE_SECTION_DEPTH: usize = 64;

pub fn load_surface_program(entry: &Path, flags: &CompilationFlags) -> Result<SurfaceProgram> {
    load_surface_program_multi_root(std::slice::from_ref(&entry.to_path_buf()), flags)
}

/// Like [`load_surface_program`] but seeds the transitive import graph from
/// zero or more roots at once instead of a single canonical entry file.
///
/// `vibra exec`'s inline program is the motivating caller: its "entry" is a
/// synthetic in-memory root assembled from `--import` flags, not a real
/// `.vibra` file, so there is no single file to canonicalize as
/// [`SurfaceProgram::entry`] -- the caller fills that field in separately.
/// `entry` on the returned program is arbitrarily the first root (or the
/// current directory if `roots` is empty) and carries no meaning for that
/// caller; every other field (`modules`, `module_parts`, `embedded_files`,
/// `imports`) is complete over the full union of the roots' transitive
/// import graphs, exactly as [`load_surface_program`] would compute it for a
/// single real entry.
pub(crate) fn load_surface_program_multi_root(
    roots: &[PathBuf],
    flags: &CompilationFlags,
) -> Result<SurfaceProgram> {
    let mut canonical_roots = Vec::with_capacity(roots.len());
    for root in roots {
        canonical_roots.push(canonical_module_path(root)?);
    }
    let project = match canonical_roots.first() {
        Some(root) => project_context::discover_project_import_context(root)?,
        None => None,
    };
    let mut modules = BTreeMap::new();
    let mut visiting = Vec::new();
    for root in &canonical_roots {
        load_recursive(root, project.as_ref(), flags, &mut modules, &mut visiting)?;
    }
    let imports = modules
        .iter()
        .map(|(path, module)| {
            Ok((
                path.clone(),
                resolved_module_imports(module, project.as_ref())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let typed_modules = modules
        .iter()
        .map(|(path, module)| {
            (
                path.clone(),
                module
                    .parts
                    .iter()
                    .map(|part| part.module.clone())
                    .collect(),
            )
        })
        .collect();
    let mut expanded = ast::expand_typed_macro_program(typed_modules, &imports)
        .context("expand typed S-expression macros")?;
    for (path, module) in &mut modules {
        let expanded_parts = expanded
            .remove(path)
            .with_context(|| format!("missing expanded module {}", path.display()))?;
        for (part, expanded_part) in module.parts.iter_mut().zip(expanded_parts) {
            part.module = expanded_part;
        }
    }
    let embedded_files = expand_compile_time_data(&mut modules)?;
    validate_unique_symbols(&modules)?;
    validate_direct_import_aliases(&modules)?;
    let module_parts = modules
        .iter()
        .map(|(path, module)| {
            (
                path.clone(),
                module.parts.iter().map(|part| part.path.clone()).collect(),
            )
        })
        .collect();
    Ok(SurfaceProgram {
        entry: canonical_roots
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from(".")),
        modules,
        module_parts,
        embedded_files,
        imports,
    })
}

pub fn load_surface_entry_for_test_discovery(entry: &Path) -> Result<(PathBuf, SourceModule)> {
    let entry = canonical_module_path(entry)?;
    let module = load_source_module(&entry, &CompilationFlags::new(["test"]))?;
    Ok((entry, module))
}

fn load_recursive(
    path: &Path,
    project: Option<&ProjectImportContext>,
    flags: &CompilationFlags,
    modules: &mut BTreeMap<PathBuf, SourceModule>,
    visiting: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = canonical_module_path(path)?;
    if modules.contains_key(&path) {
        return Ok(());
    }
    if visiting.contains(&path) {
        bail!("E-MOD-003: import cycle includes `{}`", path.display());
    }
    visiting.push(path.clone());
    let module = load_source_module(&path, flags)?;
    let imports = module_imports(&module, project)?;
    for import in imports {
        load_recursive(&import, project, flags, modules, visiting)?;
    }
    visiting.pop();
    modules.insert(path, module);
    Ok(())
}

fn load_source_module(path: &Path, flags: &CompilationFlags) -> Result<SourceModule> {
    let part_paths = module_part_paths(path, flags)?;
    let mut parts = Vec::with_capacity(part_paths.len());
    let mut symbols = BTreeSet::new();
    for part_path in part_paths {
        let source = fs::read_to_string(&part_path)
            .with_context(|| format!("read {}", part_path.display()))?;
        let document = syntax::parse(&source)
            .with_context(|| format!("parse S-expression source {}", part_path.display()))?;
        let document_id = DocumentId::from_path(&part_path);
        let module = ast::lower_document_with_id(&document, document_id)
            .with_context(|| format!("validate typed source {}", part_path.display()))?;
        for form in &module.forms {
            if let Some(name) = top_level_name(form) {
                if !symbols.insert(name.to_string()) {
                    bail!(
                        "E-MOD-002: duplicate top-level symbol `{name}` across module parts of {}",
                        path.display()
                    );
                }
            }
        }
        parts.push(SourcePart {
            path: part_path,
            source,
            document,
            module,
        });
    }
    Ok(SourceModule {
        path: path.to_path_buf(),
        parts,
    })
}

#[derive(Debug, Clone, PartialEq)]
enum CompileValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<CompileValue>),
    Record(BTreeMap<String, CompileValue>),
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

fn expand_compile_time_data(
    modules: &mut BTreeMap<PathBuf, SourceModule>,
) -> Result<BTreeMap<PathBuf, String>> {
    let mut embedded = BTreeMap::new();
    for module in modules.values_mut() {
        let package_root = crate::project::find_project_for_file(&module.path)?
            .map(|project| project.root)
            .unwrap_or_else(|| module.path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let package_root = fs::canonicalize(&package_root).with_context(|| {
            format!(
                "E-EMBED-002: resolve package root {}",
                package_root.display()
            )
        })?;
        for part in &mut module.parts {
            for form in &mut part.module.forms {
                expand_top_level(form, &part.path, &package_root, &mut embedded)?;
            }
        }
    }
    Ok(embedded)
}

/// Expand the compile-time `embed` and `template` forms of a single module,
/// resolving its package root from `module_path`.
///
/// [`load_surface_program`] runs this over a whole program that it read from
/// disk. Tooling that already holds an in-memory [`Module`] — the corpus
/// migration validator, for one — needs the same expansion without going back
/// through the loader, otherwise it reports files as failing on unexpanded
/// compile-time data that the compiler handles perfectly well.
pub fn expand_module_compile_time_data(
    module: &mut Module,
    module_path: &Path,
) -> Result<BTreeMap<PathBuf, String>> {
    let package_root = crate::project::find_project_for_file(module_path)?
        .map(|project| project.root)
        .unwrap_or_else(|| module_path.parent().unwrap_or(Path::new(".")).to_path_buf());
    let package_root = fs::canonicalize(&package_root).with_context(|| {
        format!(
            "E-EMBED-002: resolve package root {}",
            package_root.display()
        )
    })?;
    let mut embedded = BTreeMap::new();
    for form in &mut module.forms {
        expand_top_level(form, module_path, &package_root, &mut embedded)?;
    }
    Ok(embedded)
}

fn expand_top_level(
    form: &mut TopLevel,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    match form {
        TopLevel::Constant(constant) => {
            expand_expr(&mut constant.value, module_path, package_root, embedded)?;
            expand_annotations(
                &mut constant.annotations,
                module_path,
                package_root,
                embedded,
            )
        }
        TopLevel::Function(function) => {
            expand_exprs(&mut function.body, module_path, package_root, embedded)?;
            expand_annotations(
                &mut function.annotations,
                module_path,
                package_root,
                embedded,
            )
        }
        TopLevel::Test(test) => expand_exprs(&mut test.body, module_path, package_root, embedded),
        TopLevel::Definition(definition) => expand_annotations(
            &mut definition.annotations,
            module_path,
            package_root,
            embedded,
        ),
        TopLevel::Import(_) | TopLevel::Macro(_) => Ok(()),
    }
}

fn expand_annotations(
    annotations: &mut [ast::Annotation],
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    for annotation in annotations {
        match &mut annotation.value {
            AnnotationKind::Definitions(functions) => {
                for function in functions {
                    expand_exprs(&mut function.body, module_path, package_root, embedded)?;
                    expand_annotations(
                        &mut function.annotations,
                        module_path,
                        package_root,
                        embedded,
                    )?;
                }
            }
            AnnotationKind::Implementation { items, .. } => {
                for item in items {
                    if let ImplItem::Method {
                        binding: MethodBinding::Function(function),
                        ..
                    } = item
                    {
                        expand_exprs(&mut function.body, module_path, package_root, embedded)?;
                        expand_annotations(
                            &mut function.annotations,
                            module_path,
                            package_root,
                            embedded,
                        )?;
                    }
                }
            }
            AnnotationKind::Doc(_) | AnnotationKind::Where(_) => {}
        }
    }
    Ok(())
}

fn expand_exprs(
    expressions: &mut [Expr],
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    for expression in expressions {
        expand_expr(expression, module_path, package_root, embedded)?;
    }
    Ok(())
}

fn expand_expr(
    expression: &mut Expr,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    match &mut expression.value {
        ExprKind::Call { arguments, .. }
        | ExprKind::Do(arguments)
        | ExprKind::Tuple(arguments)
        | ExprKind::Array(arguments) => {
            expand_exprs(arguments, module_path, package_root, embedded)?
        }
        ExprKind::Let { value, .. }
        | ExprKind::Set { value, .. }
        | ExprKind::Mutable(value)
        | ExprKind::ReferenceOf(value) => expand_expr(value, module_path, package_root, embedded)?,
        ExprKind::Return(value) => {
            if let Some(value) = value {
                expand_expr(value, module_path, package_root, embedded)?;
            }
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            expand_expr(condition, module_path, package_root, embedded)?;
            expand_exprs(then_body, module_path, package_root, embedded)?;
            expand_exprs(else_body, module_path, package_root, embedded)?;
        }
        ExprKind::While { condition, body } => {
            expand_expr(condition, module_path, package_root, embedded)?;
            expand_exprs(body, module_path, package_root, embedded)?;
        }
        ExprKind::For { source, body, .. } => {
            expand_expr(source, module_path, package_root, embedded)?;
            expand_exprs(body, module_path, package_root, embedded)?;
        }
        ExprKind::Match { target, cases } => {
            expand_expr(target, module_path, package_root, embedded)?;
            for case in cases {
                expand_exprs(&mut case.body, module_path, package_root, embedded)?;
            }
        }
        ExprKind::Record(fields) => {
            for field in fields {
                expand_expr(&mut field.value, module_path, package_root, embedded)?;
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                expand_expr(key, module_path, package_root, embedded)?;
                expand_expr(value, module_path, package_root, embedded)?;
            }
        }
        ExprKind::Range(start, end, step) => {
            expand_expr(start, module_path, package_root, embedded)?;
            expand_expr(end, module_path, package_root, embedded)?;
            expand_expr(step, module_path, package_root, embedded)?;
        }
        ExprKind::Convert { value, .. } | ExprKind::Cast { value, .. } => {
            expand_expr(value, module_path, package_root, embedded)?
        }
        ExprKind::Template { bindings, .. } => {
            for binding in bindings.iter_mut() {
                expand_expr(&mut binding.value, module_path, package_root, embedded)?;
            }
        }
        ExprKind::Task { body, .. } => expand_exprs(body, module_path, package_root, embedded)?,
        ExprKind::Spawn { value, .. } => expand_expr(value, module_path, package_root, embedded)?,
        ExprKind::Literal(_)
        | ExprKind::Reference(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Embed { .. }
        | ExprKind::Wasm { .. }
        | ExprKind::Join { .. } => {}
    }

    let replacement = match &expression.value {
        ExprKind::Embed { path, format } => Some(load_typed_embed(
            &path.value,
            format.value,
            module_path,
            package_root,
            embedded,
            expression.span,
            expression.origin.clone(),
        )?),
        ExprKind::Template { path, bindings } => Some(load_typed_template(
            &path.value,
            bindings,
            module_path,
            package_root,
            embedded,
            expression.span,
            expression.origin.clone(),
        )?),
        _ => None,
    };
    if let Some(replacement) = replacement {
        *expression = replacement;
    }
    Ok(())
}

fn load_typed_embed(
    path: &str,
    requested_format: EmbedFormat,
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
    span: syntax::Span,
    origin: Origin,
) -> Result<Expr> {
    let resolved = resolve_package_file(
        path,
        module_path,
        package_root,
        "E-EMBED-002",
        "E-EMBED-003",
        "embedded",
    )?;
    let bytes = read_bounded_source(
        &resolved,
        MAX_EMBED_SOURCE_BYTES,
        "E-EMBED-003",
        "E-EMBED-006",
        "embedded source",
    )?;
    embedded.insert(resolved.clone(), format!("{:x}", Sha256::digest(&bytes)));
    let format = infer_embed_format(requested_format, &resolved)?;
    if format == EmbedFormat::Binary {
        return Ok(binary_compile_expr(bytes, span, origin));
    }
    let value = match format {
        EmbedFormat::Text => CompileValue::String(
            String::from_utf8(bytes).context("E-EMBED-004: text embed is not valid UTF-8")?,
        ),
        EmbedFormat::Binary => unreachable!("binary embeds are lowered directly"),
        EmbedFormat::Json => json_compile_value(
            serde_json::from_slice(&bytes).context("E-EMBED-004: parse embedded JSON")?,
        )?,
        EmbedFormat::Toml => {
            let text = std::str::from_utf8(&bytes)
                .context("E-EMBED-004: TOML embed is not valid UTF-8")?;
            toml_compile_value(toml::from_str(text).context("E-EMBED-004: parse embedded TOML")?)?
        }
        EmbedFormat::Xml => {
            let text =
                std::str::from_utf8(&bytes).context("E-EMBED-004: XML embed is not valid UTF-8")?;
            json_compile_value(
                quick_xml::de::from_str(text).context("E-EMBED-004: parse embedded XML")?,
            )?
        }
        EmbedFormat::Auto => unreachable!("auto format is resolved before expansion"),
    };
    Ok(compile_value_expr(value, span, origin))
}

fn binary_compile_expr(bytes: Vec<u8>, span: syntax::Span, origin: Origin) -> Expr {
    // Each byte narrows from the literal's inferred `int64` down to `uint8`.
    // `$cast` (`ExprKind::Cast`) is scoped to newtype<->inner-type
    // conversions only (`valid_cast_path`, `src/type_semantics.rs`); a raw
    // primitive narrowing has no valid cast path and always fails with
    // E-CAST-001. `$convert` (`ExprKind::Convert`) is the checked,
    // non-trapping numeric conversion this actually needs -- every value
    // here is a real `u8`, so it always stays in range and the fallback is
    // never observed, but the form still requires one syntactically.
    Spanned {
        value: ExprKind::Array(
            bytes
                .into_iter()
                .map(|byte| Spanned {
                    value: ExprKind::Convert {
                        value: Box::new(Spanned {
                            value: ExprKind::Literal(Literal::Int(i64::from(byte))),
                            span,
                            origin: origin.clone(),
                        }),
                        into: Spanned {
                            value: TypeExprKind::Named("uint8".to_string()),
                            span,
                            origin: origin.clone(),
                        },
                        fallback: Spanned {
                            value: Literal::Int(0),
                            span,
                            origin: origin.clone(),
                        },
                    },
                    span,
                    origin: origin.clone(),
                })
                .collect(),
        ),
        span,
        origin,
    }
}

fn infer_embed_format(requested: EmbedFormat, path: &Path) -> Result<EmbedFormat> {
    if requested != EmbedFormat::Auto {
        return Ok(requested);
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "text" | "txt" => Ok(EmbedFormat::Text),
        "binary" | "bin" => Ok(EmbedFormat::Binary),
        "json" => Ok(EmbedFormat::Json),
        "toml" => Ok(EmbedFormat::Toml),
        "xml" => Ok(EmbedFormat::Xml),
        _ => bail!(
            "E-EMBED-001: cannot infer embed format for {}; use text, binary, json, toml, or xml",
            path.display()
        ),
    }
}

fn json_compile_value(value: serde_json::Value) -> Result<CompileValue> {
    match value {
        serde_json::Value::Null => Ok(CompileValue::Null),
        serde_json::Value::Bool(value) => Ok(CompileValue::Bool(value)),
        serde_json::Value::Number(value) => {
            let source = value.to_string();
            if source
                .bytes()
                .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
            {
                let value = source
                    .parse::<f64>()
                    .context("E-EMBED-005: structured float is outside Vibra numeric range")?;
                if !value.is_finite() {
                    bail!("E-EMBED-005: structured float is outside Vibra numeric range");
                }
                Ok(CompileValue::Float(value))
            } else if let Ok(value) = source.parse::<i64>() {
                Ok(CompileValue::Int(value))
            } else {
                // Exercise the full unsigned boundary explicitly rather than
                // allowing serde_json to expose a large integer as a float.
                let _unsigned_boundary = source.parse::<u64>();
                bail!("E-EMBED-005: structured integer is outside signed 64-bit range")
            }
        }
        serde_json::Value::String(value) => Ok(CompileValue::String(value)),
        serde_json::Value::Array(values) => Ok(CompileValue::Array(
            values
                .into_iter()
                .map(json_compile_value)
                .collect::<Result<_>>()?,
        )),
        serde_json::Value::Object(values) => Ok(CompileValue::Record(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, json_compile_value(value)?)))
                .collect::<Result<_>>()?,
        )),
    }
}

fn toml_compile_value(value: toml::Value) -> Result<CompileValue> {
    match value {
        toml::Value::String(value) => Ok(CompileValue::String(value)),
        toml::Value::Integer(value) => Ok(CompileValue::Int(value)),
        toml::Value::Float(value) => Ok(CompileValue::Float(value)),
        toml::Value::Boolean(value) => Ok(CompileValue::Bool(value)),
        toml::Value::Datetime(value) => Ok(CompileValue::String(value.to_string())),
        toml::Value::Array(values) => Ok(CompileValue::Array(
            values
                .into_iter()
                .map(toml_compile_value)
                .collect::<Result<_>>()?,
        )),
        toml::Value::Table(values) => Ok(CompileValue::Record(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, toml_compile_value(value)?)))
                .collect::<Result<_>>()?,
        )),
    }
}

fn compile_value_expr(value: CompileValue, span: syntax::Span, origin: Origin) -> Expr {
    let node = |value| Spanned {
        value,
        span,
        origin: origin.clone(),
    };
    match value {
        CompileValue::Null => node(ExprKind::Literal(Literal::Unit)),
        CompileValue::Bool(value) => node(ExprKind::Literal(Literal::Bool(value))),
        CompileValue::Int(value) => node(ExprKind::Literal(Literal::Int(value))),
        CompileValue::Float(value) => node(ExprKind::Literal(Literal::Float(value))),
        CompileValue::String(value) => node(ExprKind::Literal(Literal::String(value))),
        CompileValue::Array(values) => node(ExprKind::Array(
            values
                .into_iter()
                .map(|value| compile_value_expr(value, span, origin.clone()))
                .collect(),
        )),
        CompileValue::Record(values) => node(ExprKind::Record(
            values
                .into_iter()
                .map(|(name, value)| ExprField {
                    name: Spanned {
                        value: name,
                        span,
                        origin: origin.clone(),
                    },
                    value: compile_value_expr(value, span, origin.clone()),
                    span,
                })
                .collect(),
        )),
    }
}

fn load_typed_template(
    path: &str,
    bindings: &[ExprField],
    module_path: &Path,
    package_root: &Path,
    embedded: &mut BTreeMap<PathBuf, String>,
    span: syntax::Span,
    origin: Origin,
) -> Result<Expr> {
    let resolved = resolve_package_file(
        path,
        module_path,
        package_root,
        "E-TEMPLATE-002",
        "E-TEMPLATE-003",
        "template",
    )?;
    let bytes = read_bounded_source(
        &resolved,
        MAX_TEMPLATE_SOURCE_BYTES,
        "E-TEMPLATE-003",
        "E-TEMPLATE-006",
        "template source",
    )?;
    let raw_digest = format!("{:x}", Sha256::digest(&bytes));
    let source = String::from_utf8(bytes).context("E-TEMPLATE-003: template is not valid UTF-8")?;
    let source = source.replace("\r\n", "\n");
    embedded.insert(resolved, raw_digest);
    let root = CompileValue::Record(
        bindings
            .iter()
            .map(|field| {
                Ok((
                    field.name.value.clone(),
                    compile_value_from_expr(&field.value)?,
                ))
            })
            .collect::<Result<_>>()?,
    );
    let rendered = render_template(&parse_template(&source)?, &[&root])?;
    Ok(compile_value_expr(
        CompileValue::String(rendered),
        span,
        origin,
    ))
}

fn compile_value_from_expr(expression: &Expr) -> Result<CompileValue> {
    match &expression.value {
        ExprKind::Literal(Literal::Unit) => Ok(CompileValue::Null),
        ExprKind::Literal(Literal::Bool(value)) => Ok(CompileValue::Bool(*value)),
        ExprKind::Literal(Literal::Int(value)) => Ok(CompileValue::Int(*value)),
        ExprKind::Literal(Literal::Float(value)) => Ok(CompileValue::Float(*value)),
        ExprKind::Literal(Literal::String(value)) => Ok(CompileValue::String(value.clone())),
        ExprKind::Array(values) | ExprKind::Tuple(values) => Ok(CompileValue::Array(
            values
                .iter()
                .map(compile_value_from_expr)
                .collect::<Result<_>>()?,
        )),
        ExprKind::Record(fields) => Ok(CompileValue::Record(
            fields
                .iter()
                .map(|field| {
                    Ok((
                        field.name.value.clone(),
                        compile_value_from_expr(&field.value)?,
                    ))
                })
                .collect::<Result<_>>()?,
        )),
        _ => bail!(
            "E-TEMPLATE-005: template binding `{}` must be compile-time literal data",
            expression.span.start
        ),
    }
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

fn read_bounded_source(
    path: &Path,
    limit: usize,
    read_code: &str,
    size_code: &str,
    kind: &str,
) -> Result<Vec<u8>> {
    let file = fs::File::open(path)
        .with_context(|| format!("{read_code}: read {kind} {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("{read_code}: inspect {kind} {}", path.display()))?;
    if metadata.len() > limit as u64 {
        bail!("{size_code}: {kind} exceeds {limit} bytes");
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("{read_code}: read {kind} {}", path.display()))?;
    if bytes.len() > limit {
        bail!("{size_code}: {kind} exceeds {limit} bytes");
    }
    Ok(bytes)
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
            nodes.push(TemplateNode::Variable(template_name(
                &source[content_start..close],
                "variable",
            )?));
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
        nodes.push(TemplateNode::Variable(template_name(
            tag.strip_prefix('&').unwrap_or(tag),
            "variable",
        )?));
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
            && name.split('.').any(|segment| {
                segment.is_empty()
                    || !segment
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            }))
    {
        bail!("E-TEMPLATE-004: invalid {kind} name `{name}`");
    }
    Ok(name.to_string())
}

fn render_template(nodes: &[TemplateNode], contexts: &[&CompileValue]) -> Result<String> {
    let mut output = String::new();
    render_template_into(nodes, contexts, &mut output)?;
    Ok(output)
}

fn render_template_into(
    nodes: &[TemplateNode],
    contexts: &[&CompileValue],
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
                        CompileValue::Array(items) => {
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

fn lookup_template_value<'a>(
    contexts: &[&'a CompileValue],
    name: &str,
) -> Option<&'a CompileValue> {
    if name == "." {
        return contexts.last().copied();
    }
    let mut segments = name.split('.');
    let first = segments.next()?;
    for context in contexts.iter().rev() {
        let CompileValue::Record(mapping) = context else {
            continue;
        };
        let Some(mut value) = mapping.get(first) else {
            continue;
        };
        for segment in segments.clone() {
            let CompileValue::Record(mapping) = value else {
                return None;
            };
            value = mapping.get(segment)?;
        }
        return Some(value);
    }
    None
}

fn render_template_scalar(name: &str, value: &CompileValue) -> Result<String> {
    match value {
        CompileValue::Null => Ok(String::new()),
        CompileValue::Bool(value) => Ok(value.to_string()),
        CompileValue::Int(value) => Ok(value.to_string()),
        CompileValue::Float(value) => Ok(value.to_string()),
        CompileValue::String(value) => Ok(value.clone()),
        CompileValue::Array(_) | CompileValue::Record(_) => {
            bail!("E-TEMPLATE-005: template value `{name}` must be a scalar")
        }
    }
}

fn template_truthy(value: &CompileValue) -> bool {
    match value {
        CompileValue::Null | CompileValue::Bool(false) => false,
        CompileValue::Array(items) => !items.is_empty(),
        _ => true,
    }
}

fn visit_exprs(exprs: &[Expr], visitor: &mut impl FnMut(&Expr) -> Result<()>) -> Result<()> {
    for expr in exprs {
        visit_expr(expr, visitor)?;
    }
    Ok(())
}

fn visit_expr(expr: &Expr, visitor: &mut impl FnMut(&Expr) -> Result<()>) -> Result<()> {
    visitor(expr)?;
    match &expr.value {
        ExprKind::Call { arguments, .. }
        | ExprKind::Do(arguments)
        | ExprKind::Tuple(arguments)
        | ExprKind::Array(arguments) => visit_exprs(arguments, visitor),
        ExprKind::Let { value, .. }
        | ExprKind::Set { value, .. }
        | ExprKind::Mutable(value)
        | ExprKind::ReferenceOf(value) => visit_expr(value, visitor),
        ExprKind::Return(value) => value
            .as_deref()
            .map_or(Ok(()), |value| visit_expr(value, visitor)),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            visit_expr(condition, visitor)?;
            visit_exprs(then_body, visitor)?;
            visit_exprs(else_body, visitor)
        }
        ExprKind::While { condition, body } => {
            visit_expr(condition, visitor)?;
            visit_exprs(body, visitor)
        }
        ExprKind::For { source, body, .. } => {
            visit_expr(source, visitor)?;
            visit_exprs(body, visitor)
        }
        ExprKind::Match { target, cases } => {
            visit_expr(target, visitor)?;
            for case in cases {
                visit_exprs(&case.body, visitor)?;
            }
            Ok(())
        }
        ExprKind::Record(fields) => {
            for field in fields {
                visit_expr(&field.value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                visit_expr(key, visitor)?;
                visit_expr(value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Range(start, end, step) => {
            visit_expr(start, visitor)?;
            visit_expr(end, visitor)?;
            visit_expr(step, visitor)
        }
        ExprKind::Convert { value, .. } | ExprKind::Cast { value, .. } => {
            visit_expr(value, visitor)
        }
        ExprKind::Template { bindings, .. } => {
            for binding in bindings {
                visit_expr(&binding.value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Task { body, .. } => visit_exprs(body, visitor),
        ExprKind::Spawn { value, .. } => visit_expr(value, visitor),
        ExprKind::Literal(_)
        | ExprKind::Reference(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Embed { .. }
        | ExprKind::Wasm { .. }
        | ExprKind::Join { .. } => Ok(()),
    }
}

fn module_imports(
    module: &SourceModule,
    project: Option<&ProjectImportContext>,
) -> Result<Vec<PathBuf>> {
    let mut imports = resolved_module_imports(module, project)?
        .into_values()
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    Ok(imports)
}

fn resolved_module_imports(
    module: &SourceModule,
    project: Option<&ProjectImportContext>,
) -> Result<BTreeMap<String, PathBuf>> {
    let parent = module
        .path
        .parent()
        .with_context(|| format!("{} has no parent directory", module.path.display()))?;
    let mut imports = BTreeMap::new();
    for form in module.forms() {
        let TopLevel::Import(import) = form else {
            continue;
        };
        let resolved = if import.path.value.starts_with('@') {
            let project = project.with_context(|| {
                format!(
                    "{}: @ import `{}` requires project.vibra",
                    module.path.display(),
                    import.path.value
                )
            })?;
            project_context::resolve_project_import(project, &import.path.value)?
        } else {
            parent.join(&import.path.value)
        };
        let resolved = canonical_module_path(&resolved).with_context(|| {
            format!(
                "{}: cannot resolve import `{}`",
                module.path.display(),
                import.path.value
            )
        })?;
        imports.insert(import.alias.value.clone(), resolved);
    }
    Ok(imports)
}

fn validate_unique_symbols(modules: &BTreeMap<PathBuf, SourceModule>) -> Result<()> {
    for module in modules.values() {
        let mut symbols = BTreeSet::new();
        for form in module.forms() {
            if let Some(name) = top_level_name(form) {
                if !symbols.insert(name) {
                    bail!(
                        "E-MOD-002: duplicate top-level symbol `{name}` across module parts of {}",
                        module.path.display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_direct_import_aliases(modules: &BTreeMap<PathBuf, SourceModule>) -> Result<()> {
    let known_import_aliases = modules
        .values()
        .flat_map(SourceModule::forms)
        .filter_map(|form| match form {
            TopLevel::Import(import) => Some(import.alias.value.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for module in modules.values() {
        let direct_imports = module
            .forms()
            .filter_map(|form| match form {
                TopLevel::Import(import) => Some(import.alias.value.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let local_symbols = module
            .forms()
            .filter_map(top_level_name)
            .filter(|symbol| !direct_imports.contains(symbol))
            .collect::<BTreeSet<_>>();
        let self_alias = module_self_alias(&module.path);

        for form in module.forms() {
            validate_top_level_aliases(
                form,
                &module.path,
                &known_import_aliases,
                &direct_imports,
                &local_symbols,
                self_alias.as_deref(),
            )?;
        }
    }
    Ok(())
}

fn validate_top_level_aliases(
    form: &TopLevel,
    path: &Path,
    known: &BTreeSet<&str>,
    direct: &BTreeSet<&str>,
    local: &BTreeSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    let mut validate_name =
        |name: &str| validate_reference_alias(name, path, known, direct, local, self_alias);
    match form {
        TopLevel::Definition(definition) => visit_type(&definition.body, &mut validate_name),
        TopLevel::Constant(constant) => {
            visit_type(&constant.ty, &mut validate_name)?;
            visit_expr_aliases(&constant.value, &mut validate_name)
        }
        TopLevel::Function(function) => {
            for parameter in &function.parameters {
                visit_type(&parameter.ty, &mut validate_name)?;
            }
            visit_type(&function.return_type, &mut validate_name)?;
            for expr in &function.body {
                visit_expr_aliases(expr, &mut validate_name)?;
            }
            Ok(())
        }
        TopLevel::Test(test) => {
            for expr in &test.body {
                visit_expr_aliases(expr, &mut validate_name)?;
            }
            Ok(())
        }
        TopLevel::Import(_) | TopLevel::Macro(_) => Ok(()),
    }
}

fn visit_expr_aliases(expr: &Expr, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    visit_expr(expr, &mut |expr| {
        match &expr.value {
            ExprKind::Reference(name) => visitor(name)?,
            ExprKind::Call { callee, .. } => visitor(&callee.value)?,
            ExprKind::Let { ty: Some(ty), .. } => visit_type(ty, visitor)?,
            ExprKind::Match { cases, .. } => {
                for case in cases {
                    visit_pattern(&case.pattern, visitor)?;
                }
            }
            ExprKind::Convert { into, .. } | ExprKind::Cast { into, .. } => {
                visit_type(into, visitor)?
            }
            _ => {}
        }
        Ok(())
    })
}

fn visit_type(ty: &TypeExpr, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    match &ty.value {
        TypeExprKind::Named(name) => visitor(name),
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            visitor(&constructor.value)?;
            for argument in arguments {
                visit_type(argument, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                visit_type(&member.ty, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Tuple(types)
        | TypeExprKind::Union(types)
        | TypeExprKind::Function {
            parameters: types, ..
        } => {
            for ty in types {
                visit_type(ty, visitor)?;
            }
            if let TypeExprKind::Function { result, .. } = &ty.value {
                visit_type(result, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Array(ty)
        | TypeExprKind::Newtype(ty)
        | TypeExprKind::Mutable(ty)
        | TypeExprKind::Reference(ty)
        | TypeExprKind::MutableReference(ty) => visit_type(ty, visitor),
        TypeExprKind::Map(key, value) => {
            visit_type(key, visitor)?;
            visit_type(value, visitor)
        }
        TypeExprKind::Intersect(types) => {
            for ty in types {
                visit_type(ty, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::WasmValue(domain) => visitor(&domain.value),
        TypeExprKind::Handle(_) => Ok(()),
    }
}

fn visit_pattern(pattern: &Pattern, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    match &pattern.value {
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            visitor(&constructor.value)?;
            for pattern in arguments {
                visit_pattern(pattern, visitor)?;
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                visit_pattern(&field.pattern, visitor)?;
            }
        }
        PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
            for pattern in patterns {
                visit_pattern(pattern, visitor)?;
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                visit_pattern(key, visitor)?;
                visit_pattern(value, visitor)?;
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            visit_type(ty, visitor)?;
            visit_pattern(pattern, visitor)?;
        }
        PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Bind(_) => {}
    }
    Ok(())
}

fn validate_reference_alias(
    reference: &str,
    path: &Path,
    known: &BTreeSet<&str>,
    direct: &BTreeSet<&str>,
    local: &BTreeSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    let Some((alias, _)) = reference.split_once('.') else {
        return Ok(());
    };
    if direct.contains(alias)
        || local.contains(alias)
        || self_alias == Some(alias)
        || matches!(alias, "args" | "const" | "self")
        || !known.contains(alias)
    {
        return Ok(());
    }
    bail!(
        "E-MOD-004: module {} uses import alias `{alias}` without declaring it directly",
        path.display()
    )
}

/// A module's own bare name for self-qualified references (e.g.
/// `option.vibra` calling `option.empty`). Shared with
/// `crate::load::convert_surface_program`, which needs the exact same
/// derivation when it resolves a module's own signatures under its self
/// alias for `crate::surface_adapter::resolve_signatures`.
pub(crate) fn module_self_alias(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    Some(
        file_name
            .strip_suffix(".vibra")
            .unwrap_or(file_name)
            .split('.')
            .next()
            .unwrap_or(file_name)
            .to_string(),
    )
}

fn top_level_name(form: &TopLevel) -> Option<&str> {
    match form {
        TopLevel::Import(value) => Some(&value.alias.value),
        TopLevel::Definition(value) => Some(&value.name.value),
        TopLevel::Constant(value) => Some(&value.name.value),
        TopLevel::Function(value) => Some(&value.name.value),
        TopLevel::Macro(value) => Some(&value.name.value),
        TopLevel::Test(value) => Some(&value.name.value),
    }
}

fn module_part_paths(module_path: &Path, flags: &CompilationFlags) -> Result<Vec<PathBuf>> {
    let mut paths = vec![module_path.to_path_buf()];
    let Some(parent) = module_path.parent() else {
        return Ok(paths);
    };
    let Some(file_name) = module_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(paths);
    };
    let Some(base) = file_name.strip_suffix(".vibra") else {
        bail!(
            "E-SYN-012: S-expression source must use the `.vibra` extension: {}",
            module_path.display()
        );
    };
    let prefix = format!("{base}.");
    for entry in fs::read_dir(parent).with_context(|| format!("read {}", parent.display()))? {
        let path = entry?.path();
        if path == module_path {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".vibra") else {
            continue;
        };
        let Some(suffix) = stem.strip_prefix(&prefix) else {
            continue;
        };
        let suffix_flags = suffix.split('.').collect::<Vec<_>>();
        if suffix.is_empty() || suffix_flags.iter().any(|flag| !is_compilation_flag(flag)) {
            bail!("E-FLAG-002: malformed conditional source suffix in `{file_name}`");
        }
        if suffix_flags.iter().all(|flag| flags.contains(flag)) {
            paths.push(fs::canonicalize(path)?);
        }
    }
    paths.sort();
    Ok(paths)
}

fn canonical_module_path(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 file name", path.display()))?;
    if file_name.ends_with(".vibra.yaml") {
        bail!("E-SYN-012: `.vibra.yaml` is not supported by the S-expression frontend");
    }
    if !file_name.ends_with(".vibra") {
        bail!(
            "E-SYN-012: S-expression source must use the `.vibra` extension: {}",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
    }

    #[test]
    fn loads_import_graph_and_merges_selected_parts_deterministically() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helper \"helper.vibra\")\n(fn main () void (do (helper.run)))\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(test main-runs core (do unit))\n",
        );
        write(
            &temp.path().join("main.unix.vibra"),
            "(const platform str \"unix\")\n",
        );
        write(
            &temp.path().join("helper.vibra"),
            "(fn run () void (do unit))\n",
        );
        let program = load_surface_program(&entry, &CompilationFlags::new(["test"])).unwrap();
        assert_eq!(program.modules.len(), 2);
        let entry_module = &program.modules[&program.entry];
        assert_eq!(entry_module.parts.len(), 2);
        assert_eq!(entry_module.forms().count(), 3);
        assert!(entry_module.parts[0].path.ends_with("main.test.vibra"));
        assert!(entry_module.parts[1].path.ends_with("main.vibra"));
        assert_ne!(
            entry_module.parts[0].module.document_id,
            entry_module.parts[1].module.document_id
        );
        for part in &entry_module.parts {
            assert_eq!(
                part.module.document_id,
                DocumentId::from_path(&part.path),
                "{}",
                part.path.display()
            );
        }
    }

    #[test]
    fn duplicate_symbols_across_parts_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(const answer int64 1)\n");
        write(
            &temp.path().join("main.test.vibra"),
            "(const answer int64 2)\n",
        );
        let error = load_surface_program(&entry, &CompilationFlags::new(["test"]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-002"), "{error}");
        assert!(error.contains("answer"), "{error}");
    }

    #[test]
    fn cycles_have_a_stable_error() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a.vibra");
        write(&a, "(import b \"b.vibra\")\n");
        write(&temp.path().join("b.vibra"), "(import a \"a.vibra\")\n");
        let error = load_surface_program(&a, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-003"), "{error}");
    }

    #[test]
    fn expands_typed_compile_time_data_with_origins_and_fingerprints() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(const text str (embed \"message.txt\"))\n\
             (const data dynamic (embed \"data.json\"))\n\
             (const config dynamic (embed \"config.toml\"))\n\
             (const document dynamic (embed \"document.xml\"))\n\
             (const bytes dynamic (embed \"payload.bin\"))\n\
             (const rendered str\n\
               (template \"message.mustache\"\n\
                 with: (record (name \"Vibra\") (items (array 1 2)))))\n",
        );
        write(&temp.path().join("message.txt"), "hello");
        write(&temp.path().join("data.json"), r#"{"ok":true,"count":2}"#);
        write(&temp.path().join("config.toml"), "enabled = true");
        write(
            &temp.path().join("document.xml"),
            "<root><name>Vibra</name></root>",
        );
        fs::write(temp.path().join("payload.bin"), [0_u8, 127, 255]).unwrap();
        write(
            &temp.path().join("message.mustache"),
            "Hello {{name}}:{{#items}} {{.}}{{/items}}",
        );

        let program = load_surface_program(&entry, &CompilationFlags::default()).unwrap();
        assert_eq!(program.embedded_files.len(), 6);
        let module = &program.modules[&program.entry];
        let document = DocumentId::from_path(&program.entry);
        let values = module
            .forms()
            .filter_map(|form| match form {
                TopLevel::Constant(constant) => Some(&constant.value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            values[0].value,
            ExprKind::Literal(Literal::String(ref value)) if value == "hello"
        ));
        assert!(matches!(values[1].value, ExprKind::Record(_)));
        assert!(matches!(
            values[5].value,
            ExprKind::Literal(Literal::String(ref value)) if value == "Hello Vibra: 1 2"
        ));
        assert!(matches!(values[2].value, ExprKind::Record(_)));
        assert!(matches!(values[3].value, ExprKind::Record(_)));
        // Each embedded byte narrows `int64` -> `uint8` with `Convert`, the
        // checked numeric conversion. It was `Cast` until a binary embed was
        // actually compiled: `Cast` is scoped to newtype<->inner-type
        // conversions, so a raw primitive narrowing has no valid cast path and
        // always failed `E-CAST-001`.
        assert!(matches!(
            values[4].value,
            ExprKind::Array(ref values)
                if matches!(
                    values[2].value,
                    ExprKind::Convert {
                        ref value,
                        ref into,
                        ref fallback,
                    } if matches!(value.value, ExprKind::Literal(Literal::Int(255)))
                        && matches!(into.value, TypeExprKind::Named(ref name) if name == "uint8")
                        && matches!(fallback.value, Literal::Int(0))
                )
        ));
        assert!(
            values
                .iter()
                .all(|value| value.origin.document_id() == Some(document)),
            "expected {document:?}, got {:?}",
            values
                .iter()
                .map(|value| value.origin.document_id())
                .collect::<Vec<_>>()
        );

        let old = program.embedded_files
            [&fs::canonicalize(temp.path().join("message.txt")).unwrap()]
            .clone();
        write(&temp.path().join("message.txt"), "changed");
        let changed = load_surface_program(&entry, &CompilationFlags::default()).unwrap();
        assert_ne!(
            old,
            changed.embedded_files[&fs::canonicalize(temp.path().join("message.txt")).unwrap()]
        );
    }

    #[test]
    fn typed_compile_time_data_preserves_package_sandbox() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("package");
        fs::create_dir(&package).unwrap();
        let entry = package.join("main.vibra");
        write(&entry, "(const secret str (embed \"../secret.txt\"))\n");
        write(&root.path().join("secret.txt"), "secret");
        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-EMBED-002"), "{error}");
    }

    #[test]
    fn typed_compile_time_data_rejects_unsupported_inputs_explicitly() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&temp.path().join("data.unknown"), "value");
        write(&entry, "(const value dynamic (embed \"data.unknown\"))\n");
        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-EMBED-001"), "{error}");

        write(&temp.path().join("message.mustache"), "{{name}}");
        write(
            &entry,
            "(const value str\n\
               (template \"message.mustache\" with: (record (name runtime-value))))\n",
        );
        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-TEMPLATE-005"), "{error}");
    }

    #[test]
    fn typed_json_integers_preserve_signed_boundaries_without_float_coercion() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        let data = temp.path().join("data.json");
        write(&entry, "(const value dynamic (embed \"data.json\"))\n");
        write(&data, "[-9223372036854775808,9223372036854775807]");
        let program = load_surface_program(&entry, &CompilationFlags::default()).unwrap();
        let TopLevel::Constant(constant) = program.modules[&program.entry].forms().next().unwrap()
        else {
            panic!("expected constant");
        };
        assert!(matches!(
            constant.value.value,
            ExprKind::Array(ref values)
                if matches!(
                    values.as_slice(),
                    [
                        Spanned {
                            value: ExprKind::Literal(Literal::Int(i64::MIN)),
                            ..
                        },
                        Spanned {
                            value: ExprKind::Literal(Literal::Int(i64::MAX)),
                            ..
                        }
                    ]
                )
        ));

        for source in [
            "9223372036854775808",
            "18446744073709551615",
            "-9223372036854775809",
        ] {
            write(&data, source);
            let error = load_surface_program(&entry, &CompilationFlags::default())
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("E-EMBED-005: structured integer is outside signed 64-bit range"),
                "{source}: {error}"
            );
        }
    }

    #[test]
    fn typed_compile_time_inputs_are_bounded_before_expansion() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        let oversized = vec![b'x'; MAX_EMBED_SOURCE_BYTES + 1];

        write(&entry, "(const value str (embed \"large.txt\"))\n");
        fs::write(temp.path().join("large.txt"), &oversized).unwrap();
        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-EMBED-006"), "{error}");

        write(
            &entry,
            "(const value str (template \"large.mustache\" with: (record)))\n",
        );
        fs::write(temp.path().join("large.mustache"), oversized).unwrap();
        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-TEMPLATE-006"), "{error}");
    }

    #[test]
    fn template_rendering_normalizes_crlf_but_fingerprints_raw_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        let template = temp.path().join("message.mustache");
        write(
            &entry,
            "(const value str\n\
               (template \"message.mustache\" with: (record (name \"Vibra\"))))\n",
        );
        let crlf = b"Hello\r\n{{name}}\r\n";
        fs::write(&template, crlf).unwrap();
        let first = load_surface_program(&entry, &CompilationFlags::default()).unwrap();
        let canonical = fs::canonicalize(&template).unwrap();
        assert_eq!(
            first.embedded_files[&canonical],
            format!("{:x}", Sha256::digest(crlf))
        );
        let TopLevel::Constant(first_value) = first.modules[&first.entry].forms().next().unwrap()
        else {
            panic!("expected constant");
        };
        assert!(matches!(
            first_value.value.value,
            ExprKind::Literal(Literal::String(ref value)) if value == "Hello\nVibra\n"
        ));

        let lf = b"Hello\n{{name}}\n";
        fs::write(&template, lf).unwrap();
        let second = load_surface_program(&entry, &CompilationFlags::default()).unwrap();
        assert_eq!(
            second.embedded_files[&canonical],
            format!("{:x}", Sha256::digest(lf))
        );
        assert_ne!(
            first.embedded_files[&canonical],
            second.embedded_files[&canonical]
        );
        let TopLevel::Constant(second_value) =
            second.modules[&second.entry].forms().next().unwrap()
        else {
            panic!("expected constant");
        };
        assert_eq!(first_value.value.value, second_value.value.value);
    }

    #[test]
    fn expands_local_cross_part_and_imported_macros_before_validation() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helpers \"helpers.vibra\")\n\
             (fn main ((caller int64)) int64\n\
               (do (local caller) (helpers.use-helper caller)))\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(macro local ((value expr-syntax)) expr-syntax\n\
               (do (quote expr-syntax (unquote value))))\n",
        );
        write(
            &temp.path().join("helpers.vibra"),
            "(fn helper ((value int64)) int64 (do value))\n\
             (macro use-helper ((value expr-syntax)) expr-syntax\n\
               (do (quote expr-syntax (helper (unquote value)))))\n",
        );
        let program = load_surface_program(&entry, &CompilationFlags::new(["test"])).unwrap();
        let module = &program.modules[&program.entry];
        assert!(module
            .forms()
            .all(|form| !matches!(form, TopLevel::Macro(_))));
        let TopLevel::Function(function) = module
            .forms()
            .find(|form| matches!(form, TopLevel::Function(_)))
            .unwrap()
        else {
            unreachable!()
        };
        let ExprKind::Call { callee, .. } = &function.body[1].value else {
            panic!("expected imported macro expansion call");
        };
        assert_eq!(callee.value, "helpers.helper");
    }

    #[test]
    fn imported_private_macros_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helpers \"helpers.vibra\")\n\
             (fn main () void (do (helpers.secret unit)))\n",
        );
        write(
            &temp.path().join("helpers.vibra"),
            "(private (macro secret ((value expr-syntax)) expr-syntax\n\
               (do (unquote value))))\n",
        );
        let error = format!(
            "{:#}",
            load_surface_program(&entry, &CompilationFlags::default()).unwrap_err()
        );
        assert!(error.contains("E-MACRO-010"), "{error}");
    }

    #[test]
    fn entry_discovery_selects_test_parts_without_import_loading() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(import missing \"missing.vibra\")\n");
        write(
            &temp.path().join("main.test.vibra"),
            "(test selected core (do unit))\n",
        );
        let (_, module) = load_surface_entry_for_test_discovery(&entry).unwrap();
        assert_eq!(module.parts.len(), 2);
        assert!(module.forms().any(|form| matches!(form, TopLevel::Test(_))));
    }

    #[test]
    fn rejects_transitive_import_alias_references() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import middle \"middle.vibra\")\n(fn main () void (do (leaf.run)))\n",
        );
        write(
            &temp.path().join("middle.vibra"),
            "(import leaf \"leaf.vibra\")\n(fn run () void (do (leaf.run)))\n",
        );
        write(
            &temp.path().join("leaf.vibra"),
            "(fn run () void (do unit))\n",
        );

        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-004"), "{error}");
        assert!(error.contains("leaf"), "{error}");
    }
}
