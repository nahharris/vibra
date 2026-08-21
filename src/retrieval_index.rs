//! Deterministic, function-level retrieval index generation.
//!
//! The index deliberately stops at normalized source and compiler-known
//! semantic relations. It does not embed, rank, or persist vectors. A future
//! effect-inference pass populates the `effects.inferred` field without
//! changing the record shape used by consumers today.

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::ast::{AnnotationKind, Origin, TopLevel, Visibility};
use crate::frontend::{self, SurfaceProgram};
use crate::load::{self, CompilationFlags};
use crate::lower::{
    self, EffectRow, Expr, FunctionBody, FunctionSig, HandleAccess, Statement, TypeRef,
};
use crate::syntax::Span;

const INDEX_VERSION: u64 = 1;

#[derive(Debug, Clone)]
struct SurfaceFunction {
    module_path: PathBuf,
    source_part: PathBuf,
    source: String,
    span: Span,
    visibility: Visibility,
    origin: Origin,
}

#[derive(Debug, Default)]
struct SurfaceFunctions {
    functions: BTreeMap<String, SurfaceFunction>,
    aliases: BTreeMap<String, PathBuf>,
}

/// Build the normalized retrieval index for an entry module.
pub fn build(entry: &Path, flags: &CompilationFlags) -> Result<Value> {
    let surface = frontend::load_surface_program(entry, flags)?;
    let loaded = load::load_legacy_yaml_program(entry, flags)?;
    let lowered = lower::lower_program(&loaded)?;
    let surface_functions = collect_surface_functions(&surface)?;
    let root = corpus_root(&surface.entry, &surface.modules)?;
    let inference = crate::effect_semantics::infer_program(&lowered.functions);

    let mut records = Vec::new();
    let mut functions = lowered.functions.iter().collect::<Vec<_>>();
    functions.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (key, signature) in functions {
        records.push(build_record(
            key,
            signature,
            &surface_functions,
            &root,
            &surface.entry,
            inference
                .functions
                .get(key)
                .with_context(|| format!("effect inference missing `{key}`"))?,
        )?);
    }

    let main_signature = main_signature(&lowered);
    let main_inferred =
        crate::effect_semantics::infer_statements(&lowered.statements, &lowered.functions);
    records.push(build_record(
        "main",
        &main_signature,
        &surface_functions,
        &root,
        &surface.entry,
        &main_inferred,
    )?);
    records.sort_by(|left, right| {
        let left_name = left["qualified-name"].as_str().unwrap_or_default();
        let right_name = right["qualified-name"].as_str().unwrap_or_default();
        left_name.cmp(right_name).then_with(|| {
            left["module-path"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["module-path"].as_str().unwrap_or_default())
        })
    });

    Ok(sorted_json(object([
        ("records", Value::Array(records)),
        ("version", Value::Number(INDEX_VERSION.into())),
    ])))
}

/// Serialize an index with recursively sorted object keys.
pub fn canonical_json(index: &Value) -> Result<String> {
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&sorted_json(index.clone()))?
    ))
}

fn build_record(
    key: &str,
    signature: &FunctionSig,
    surface: &SurfaceFunctions,
    root: &Path,
    entry: &Path,
    inferred: &crate::effect_semantics::InferredEffects,
) -> Result<Value> {
    let metadata = surface.functions.get(key);
    let generated =
        metadata.map_or(true, |value| origin_is_generated(&value.origin)) || is_synthetic_key(key);
    let origin = metadata
        .map(|value| origin_value(&value.origin))
        .unwrap_or_else(|| object([(String::from("kind"), Value::String("generated".into()))]));
    let module = metadata
        .map(|value| value.module_path.clone())
        .unwrap_or_else(|| {
            surface
                .aliases
                .get(&signature.alias)
                .cloned()
                .unwrap_or_else(|| entry.to_path_buf())
        });
    let source = if generated {
        String::new()
    } else {
        normalized_function_source(metadata.expect("non-generated function has metadata"))?
    };
    let visibility = metadata
        .map(|value| value.visibility)
        .unwrap_or_else(|| visibility_from_key(key));
    let declared = effect_labels(&signature.effects);
    let inferred_labels: Vec<_> = inferred
        .labels()
        .into_iter()
        .map(|label| format!("{}.{}", label.0, label.1))
        .map(Value::String)
        .collect();

    Ok(sorted_json(object([
        (
            "call-edges",
            Value::Array(
                collect_call_edges(signature)
                    .into_iter()
                    .map(|edge| Value::String(display_qualified_name(&edge)))
                    .collect(),
            ),
        ),
        (
            "effects",
            sorted_json(object([
                ("declared", Value::Array(declared)),
                ("inferred", Value::Array(inferred_labels.clone())),
                ("performed", Value::Array(inferred_labels)),
            ])),
        ),
        (
            "error-types",
            Value::Array(
                error_types(&signature.return_type)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        ("generated", Value::Bool(generated)),
        ("module-path", Value::String(display_path(&module, root))),
        ("origin", origin),
        ("qualified-name", Value::String(display_qualified_name(key))),
        ("signature", signature_value(signature)),
        ("source", Value::String(source)),
        (
            "visibility",
            Value::String(match visibility {
                Visibility::Public => "public".into(),
                Visibility::Private => "private".into(),
            }),
        ),
    ])))
}

fn main_signature(program: &lower::LoweredProgram) -> FunctionSig {
    let parameters = program
        .main_arg_bindings
        .iter()
        .filter(|(name, _)| name.starts_with("args.") && !name[5..].contains('.'))
        .map(|(name, ty)| lower::Parameter {
            name: name[5..].to_string(),
            ty: ty.clone(),
            variadic: false,
        })
        .collect();
    FunctionSig {
        alias: String::new(),
        symbol: "main".into(),
        visibility: lower::FunctionVisibility::Public,
        interface_method: false,
        owner_effect: None,
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        parameters,
        return_type: TypeRef::Void,
        body: FunctionBody::User {
            statements: program.statements.clone(),
        },
        doc: None,
        effects: program.main_effects.clone(),
    }
}

fn collect_surface_functions(program: &SurfaceProgram) -> Result<SurfaceFunctions> {
    let mut collected = SurfaceFunctions::default();
    let mut visited = HashSet::new();
    let mut pending = VecDeque::new();
    pending.push_back((String::new(), program.entry.clone()));
    while let Some((alias, module_path)) = pending.pop_front() {
        if !visited.insert((alias.clone(), module_path.clone())) {
            continue;
        }
        let module = program
            .modules
            .get(&module_path)
            .with_context(|| format!("typed module {} is missing", module_path.display()))?;
        collected.aliases.insert(alias.clone(), module_path.clone());
        for part in &module.parts {
            for form in &part.module.forms {
                if let TopLevel::Import(import) = form {
                    if let Some(target) = program
                        .imports
                        .get(&module_path)
                        .and_then(|imports| imports.get(&import.alias.value))
                    {
                        pending.push_back((qualify(&alias, &import.alias.value), target.clone()));
                    }
                }
            }
        }
        for part in &module.parts {
            for form in &part.module.forms {
                collect_form_functions(
                    &alias,
                    &module_path,
                    &part.path,
                    &part.source,
                    form,
                    &mut collected.functions,
                );
            }
        }
    }
    Ok(collected)
}

fn collect_form_functions(
    alias: &str,
    module_path: &Path,
    source_part: &Path,
    source: &str,
    form: &TopLevel,
    functions: &mut BTreeMap<String, SurfaceFunction>,
) {
    let mut add = |key: String, visibility: Visibility, span: Span, origin: Origin| {
        functions.entry(key).or_insert_with(|| SurfaceFunction {
            module_path: module_path.to_path_buf(),
            source_part: source_part.to_path_buf(),
            source: source.to_string(),
            span,
            visibility,
            origin,
        });
    };
    match form {
        TopLevel::Function(function) => add(
            qualify_visible(alias, &function.name.value, function.visibility),
            function.visibility,
            function.span,
            function.origin.clone(),
        ),
        TopLevel::Deffect(deffect) => {
            for operation in &deffect.operations {
                let symbol = format!("{}.{}", deffect.name.value, operation.function.name.value);
                add(
                    qualify_visible(alias, &symbol, operation.function.visibility),
                    operation.function.visibility,
                    operation.function.span,
                    operation.function.origin.clone(),
                );
            }
        }
        TopLevel::Definition(definition) => {
            for annotation in &definition.annotations {
                let AnnotationKind::Definitions(definitions) = &annotation.value else {
                    continue;
                };
                for function in definitions {
                    let symbol = format!("{}.{}", definition.name.value, function.name.value);
                    let visibility = if definition.visibility == Visibility::Private {
                        Visibility::Private
                    } else {
                        function.visibility
                    };
                    add(
                        qualify_visible(alias, &symbol, visibility),
                        visibility,
                        function.span,
                        function.origin.clone(),
                    );
                }
            }
            // Inline implementation functions are lowered through generated
            // forwarding helpers. They intentionally do not get an authored
            // source span here; the unmatched lowered record takes the honest
            // generated fallback in `build_record`.
        }
        TopLevel::Import(_)
        | TopLevel::Constant(_)
        | TopLevel::Macro(_)
        | TopLevel::Test(_)
        | TopLevel::TestScenario(_) => {}
    }
}

fn normalized_function_source(function: &SurfaceFunction) -> Result<String> {
    let start = function.span.start;
    let end = function.span.end;
    if start > end || end > function.source.len() {
        return Ok(String::new());
    }
    let source = &function.source[start..end];
    crate::tooling::format_source(&function.source_part, source).with_context(|| {
        format!(
            "format retrieval-index source for {} bytes {}..{}",
            function.source_part.display(),
            start,
            end
        )
    })
}

fn corpus_root(
    entry: &Path,
    modules: &BTreeMap<PathBuf, frontend::SourceModule>,
) -> Result<PathBuf> {
    if let Some(project) = crate::project::find_project_for_file(entry)? {
        return Ok(project.root);
    }
    let current = std::env::current_dir().context("resolve current directory")?;
    if entry.starts_with(&current) {
        return Ok(current);
    }
    let mut root = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
    for path in modules.keys() {
        while !path.starts_with(&root) {
            if !root.pop() {
                return Ok(PathBuf::from("."));
            }
        }
    }
    Ok(root)
}

fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

fn qualify_visible(alias: &str, symbol: &str, visibility: Visibility) -> String {
    let symbol = match visibility {
        Visibility::Public => symbol.to_string(),
        Visibility::Private => format!("-{symbol}"),
    };
    qualify(alias, &symbol)
}

fn display_qualified_name(key: &str) -> String {
    if key.starts_with("-adapt-") {
        return key.to_string();
    }
    key.split('.')
        .map(|segment| segment.strip_prefix('-').unwrap_or(segment))
        .collect::<Vec<_>>()
        .join(".")
}

fn is_synthetic_key(key: &str) -> bool {
    key.starts_with("-adapt-")
}

fn visibility_from_key(key: &str) -> Visibility {
    if key.split('.').any(|segment| segment.starts_with('-')) {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn origin_is_generated(origin: &Origin) -> bool {
    matches!(
        origin,
        Origin::Expansion { .. } | Origin::DocumentExpansion { .. }
    )
}

fn origin_value(origin: &Origin) -> Value {
    match origin {
        Origin::Source(_) | Origin::DocumentSource { .. } => {
            object([(String::from("kind"), Value::String("source".into()))])
        }
        Origin::Expansion {
            call_site,
            definition,
            ..
        } => object([
            (String::from("call-site"), source_location(0, *call_site)),
            (
                String::from("definition-site"),
                source_location(0, *definition),
            ),
            (String::from("kind"), Value::String("expansion".into())),
        ]),
        Origin::DocumentExpansion {
            call_site,
            definition,
            ..
        } => object([
            (
                String::from("call-site"),
                source_location(call_site.document.raw(), call_site.span),
            ),
            (
                String::from("definition-site"),
                source_location(definition.document.raw(), definition.span),
            ),
            (String::from("kind"), Value::String("expansion".into())),
        ]),
    }
}

fn source_location(document: u64, span: Span) -> Value {
    object([
        (String::from("document"), Value::Number(document.into())),
        (String::from("end"), Value::Number((span.end as u64).into())),
        (
            String::from("start"),
            Value::Number((span.start as u64).into()),
        ),
    ])
}

fn effect_labels(row: &EffectRow) -> Vec<Value> {
    row.labels
        .iter()
        .map(|(domain, action)| Value::String(format!("{domain}.{action}")))
        .collect()
}

fn signature_value(signature: &FunctionSig) -> Value {
    let type_parameters = signature
        .type_params
        .iter()
        .enumerate()
        .map(|(index, name)| {
            object([
                (
                    String::from("bounds"),
                    Value::Array(
                        signature
                            .type_param_bounds
                            .get(index)
                            .into_iter()
                            .flatten()
                            .map(|bound| Value::String(type_text(bound)))
                            .collect(),
                    ),
                ),
                (String::from("name"), Value::String(name.clone())),
            ])
        })
        .collect();
    let parameters = signature
        .parameters
        .iter()
        .map(|parameter| {
            object([
                (String::from("name"), Value::String(parameter.name.clone())),
                (
                    String::from("type"),
                    Value::String(type_text(&parameter.ty)),
                ),
                (String::from("variadic"), Value::Bool(parameter.variadic)),
            ])
        })
        .collect();
    sorted_json(object([
        (String::from("parameters"), Value::Array(parameters)),
        (
            String::from("return"),
            Value::String(type_text(&signature.return_type)),
        ),
        (
            String::from("type-parameters"),
            Value::Array(type_parameters),
        ),
    ]))
}

fn error_types(return_type: &TypeRef) -> BTreeSet<String> {
    let mut errors = BTreeSet::new();
    collect_error_types(return_type, &mut errors);
    errors
}

fn collect_error_types(ty: &TypeRef, errors: &mut BTreeSet<String>) {
    match ty {
        TypeRef::Instantiated { base, type_args }
            if base.rsplit('.').next() == Some("result") && type_args.len() >= 2 =>
        {
            collect_error_payload(&type_args[1], errors);
        }
        TypeRef::Union(items) | TypeRef::Intersect(items) | TypeRef::Tuple(items) => {
            for item in items {
                collect_error_types(item, errors);
            }
        }
        TypeRef::Array(inner)
        | TypeRef::Mutable(inner)
        | TypeRef::JoinHandle(inner)
        | TypeRef::Newtype { inner, .. }
        | TypeRef::Reference { inner, .. } => collect_error_types(inner, errors),
        TypeRef::Map { key, value } => {
            collect_error_types(key, errors);
            collect_error_types(value, errors);
        }
        _ => {}
    }
}

fn collect_error_payload(ty: &TypeRef, errors: &mut BTreeSet<String>) {
    match ty {
        TypeRef::Union(items) | TypeRef::Intersect(items) | TypeRef::Tuple(items) => {
            for item in items {
                collect_error_payload(item, errors);
            }
        }
        TypeRef::Named(_) | TypeRef::Instantiated { .. } | TypeRef::Generic(_) => {
            errors.insert(type_text(ty));
        }
        _ => {}
    }
}

fn type_text(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Atom => "atom".into(),
        TypeRef::Bool => "bool".into(),
        TypeRef::Str => "str".into(),
        TypeRef::Int8 => "int8".into(),
        TypeRef::Int16 => "int16".into(),
        TypeRef::Int32 => "int32".into(),
        TypeRef::Int64 => "int64".into(),
        TypeRef::UInt8 => "uint8".into(),
        TypeRef::UInt16 => "uint16".into(),
        TypeRef::UInt32 => "uint32".into(),
        TypeRef::UInt64 => "uint64".into(),
        TypeRef::Float32 => "float32".into(),
        TypeRef::Float64 => "float64".into(),
        TypeRef::Void => "void".into(),
        TypeRef::Range => "range".into(),
        TypeRef::Named(name) | TypeRef::Generic(name) => name.clone(),
        TypeRef::Newtype { name, inner } => {
            if name.is_empty() {
                format!("(newtype {})", type_text(inner))
            } else {
                name.clone()
            }
        }
        TypeRef::Mutable(inner) => format!("(mut {})", type_text(inner)),
        TypeRef::Reference { inner, mutable } => {
            if *mutable {
                format!("(ref-mut {})", type_text(inner))
            } else {
                format!("(ref {})", type_text(inner))
            }
        }
        TypeRef::HostHandle(access) => format!("handle.{}", handle_access_text(*access)),
        TypeRef::JoinHandle(inner) => format!("(join-handle {})", type_text(inner)),
        TypeRef::Instantiated { base, type_args } => {
            format!("({base} {})", type_list(type_args))
        }
        TypeRef::Literal(literal) => format!("(literal {literal:?})"),
        TypeRef::Record(fields) => format!(
            "(record {})",
            fields
                .iter()
                .map(|(name, ty)| format!("({name} {})", type_text(ty)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        TypeRef::Tuple(items) => format!("(tuple {})", type_list(items)),
        TypeRef::Array(inner) => format!("(array {})", type_text(inner)),
        TypeRef::Map { key, value } => {
            format!("(map {} {})", type_text(key), type_text(value))
        }
        TypeRef::Union(items) => format!("(union {})", type_list(items)),
        TypeRef::Enum(fields) => format!(
            "(enum {})",
            fields
                .iter()
                .map(|(name, ty)| format!("({name} {})", type_text(ty)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        TypeRef::Interface(fields) => format!(
            "(interface {})",
            fields
                .iter()
                .map(|(name, ty)| format!("({name} {})", type_text(ty)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        TypeRef::Intersect(items) => format!("(intersect {})", type_list(items)),
        TypeRef::FnType {
            parameters,
            return_type,
            effects,
        } => {
            let args = parameters
                .iter()
                .map(|parameter| {
                    format!(
                        "({}{} {})",
                        parameter.name,
                        if parameter.variadic { "..." } else { "" },
                        type_text(&parameter.ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            let effects = effects
                .iter()
                .map(|(domain, action)| format!("{domain}.{action}"))
                .collect::<Vec<_>>()
                .join(" ");
            if effects.is_empty() {
                format!("(fn-type ({args}) {})", type_text(return_type))
            } else {
                format!(
                    "(fn-type ({args}) {} effects: ({}))",
                    type_text(return_type),
                    effects
                )
            }
        }
        TypeRef::SelfType => "self".into(),
    }
}

fn type_list(items: &[TypeRef]) -> String {
    items.iter().map(type_text).collect::<Vec<_>>().join(" ")
}

fn handle_access_text(access: HandleAccess) -> &'static str {
    match access {
        HandleAccess::Read => "read",
        HandleAccess::Write => "write",
        HandleAccess::ReadWrite => "read-write",
        HandleAccess::Any => "any",
        HandleAccess::Process => "process",
    }
}

fn collect_call_edges(signature: &FunctionSig) -> BTreeSet<String> {
    let mut edges = BTreeSet::new();
    if let FunctionBody::User { statements } = &signature.body {
        collect_statement_calls(statements, &mut edges);
    }
    edges
}

fn collect_statement_calls(statements: &[Statement], edges: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            Statement::Call(call) => collect_call(call, edges),
            Statement::Let { value, .. } => match value {
                lower::LetValue::Call(call) => collect_call(call, edges),
                lower::LetValue::Expr(expr) => collect_expr_calls(expr, edges),
            },
            Statement::Set { value, .. } | Statement::Return(value) | Statement::Eval(value) => {
                collect_expr_calls(value, edges)
            }
            Statement::Match { target, arms } => {
                collect_expr_calls(target, edges);
                for arm in arms {
                    collect_statement_calls(&arm.body, edges);
                }
            }
            Statement::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_calls(cond, edges);
                collect_statement_calls(then_body, edges);
                collect_statement_calls(else_body, edges);
            }
            Statement::While { cond, body } => {
                collect_expr_calls(cond, edges);
                collect_statement_calls(body, edges);
            }
            Statement::For { source, body, .. } => {
                collect_expr_calls(source, edges);
                collect_statement_calls(body, edges);
            }
            Statement::Task { body, .. } => collect_statement_calls(body, edges),
            Statement::Spawn { value, .. } => collect_expr_calls(value, edges),
            Statement::Join { .. } | Statement::Break | Statement::Continue => {}
        }
    }
}

fn collect_call(call: &lower::Call, edges: &mut BTreeSet<String>) {
    edges.insert(call.callee_key.clone());
    for argument in &call.args {
        collect_expr_calls(argument, edges);
    }
}

fn collect_expr_calls(expr: &Expr, edges: &mut BTreeSet<String>) {
    match expr {
        Expr::Call { call, .. } => collect_call(call, edges),
        Expr::Mutable(value) | Expr::Cast { from: value, .. } => collect_expr_calls(value, edges),
        Expr::Try { inner, .. } => collect_expr_calls(inner, edges),
        Expr::Reference { target, .. } => collect_expr_calls(target, edges),
        Expr::Primitive { args, .. } | Expr::Tuple(args) | Expr::Array(args) => {
            for argument in args {
                collect_expr_calls(argument, edges);
            }
        }
        Expr::EnumConstructor { payload, .. } => {
            if let Some(payload) = payload {
                collect_expr_calls(payload, edges);
            }
        }
        Expr::Record(fields) => {
            for value in fields.values() {
                collect_expr_calls(value, edges);
            }
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                collect_expr_calls(key, edges);
                collect_expr_calls(value, edges);
            }
        }
        Expr::Range { start, end, step } => {
            collect_expr_calls(start, edges);
            collect_expr_calls(end, edges);
            collect_expr_calls(step, edges);
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            collect_expr_calls(cond, edges);
            collect_expr_calls(then_e, edges);
            collect_expr_calls(else_e, edges);
        }
        Expr::HostCall { args, .. } => {
            for argument in args {
                collect_expr_calls(argument, edges);
            }
        }
        Expr::Value(_) | Expr::VarRef(_) => {}
    }
}

fn object<K>(entries: impl IntoIterator<Item = (K, Value)>) -> Value
where
    K: Into<String>,
{
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn sorted_json(value: Value) -> Value {
    match value {
        Value::Object(entries) => {
            let entries = entries
                .into_iter()
                .map(|(key, value)| (key, sorted_json(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(entries.into_iter().collect::<Map<_, _>>())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sorted_json).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Origin;
    use std::sync::Arc;

    #[test]
    fn expansion_origin_uses_generated_source_fallback() {
        let key = "generated";
        let source = "(defn generated () void (do (return)))";
        let origin = Origin::Expansion {
            call_site: Span::new(2, 4),
            definition: Span::new(10, 20),
            parent: Arc::new(Origin::Source(Span::new(0, source.len()))),
        };
        let mut surface = SurfaceFunctions::default();
        surface.functions.insert(
            key.to_string(),
            SurfaceFunction {
                module_path: PathBuf::from("src/main.vib"),
                source_part: PathBuf::from("src/main.vib"),
                source: source.to_string(),
                span: Span::new(0, source.len()),
                visibility: Visibility::Public,
                origin: origin.clone(),
            },
        );
        let signature = FunctionSig {
            alias: String::new(),
            symbol: key.to_string(),
            visibility: lower::FunctionVisibility::Public,
            interface_method: false,
            owner_effect: None,
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            parameters: Vec::new(),
            return_type: TypeRef::Void,
            body: FunctionBody::User {
                statements: Vec::new(),
            },
            doc: None,
            effects: EffectRow::default(),
        };

        let record = build_record(
            key,
            &signature,
            &surface,
            Path::new("."),
            Path::new("src/main.vib"),
            &crate::effect_semantics::InferredEffects::default(),
        )
        .unwrap();

        assert!(origin_is_generated(&origin));
        assert_eq!(record["generated"], true);
        assert_eq!(record["source"], "");
        assert_eq!(record["origin"]["kind"], "expansion");
    }
}
