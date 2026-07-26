//! Executable lowering from typed S-expression AST nodes.
//!
//! This is separate from `typed_lower`: declarations become executable only
//! after every expression in their body has lowered successfully.

use crate::ast::{
    AnnotationKind, Expr as AstExpr, ExprKind, Function, Literal, Module, Origin,
    Pattern as AstPattern, PatternKind, SourceLocation, TopLevel, WasmArgument,
};
use crate::body_semantics::{validate_function_body, validate_task_handles};
use crate::lower::{
    infer_expr_type, primitive_integer, primitive_numeric, typed_primitive_op, Call, EnumDef, Expr,
    FunctionBody, FunctionSig, ImportTarget, LetValue, MatchArm, Pattern, PrimitiveOp,
    RuntimeValue, Statement, TypeAlias, TypeRef, WasmArgSpec,
};
use crate::type_semantics::{capability_body, policy_body, policy_type_is_subset, type_compatible};
use crate::typed_lower::{
    lower_type, qualify, TypedFunctionSignature, TypedModuleInput, TypedSignatureIndex,
};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default)]
pub struct TypedBodyIndex {
    // Deliberately private: these are staged IR fragments, not executable
    // functions. Only the checked materializer below may expose FunctionSig.
    functions: HashMap<String, FunctionBody>,
    constants: HashMap<String, Expr>,
    tests: HashMap<String, Vec<Statement>>,
    origins: HashMap<String, SourceLocation>,
    node_origins: HashMap<String, Vec<Origin>>,
    let_types: HashMap<String, HashMap<String, TypeRef>>,
    lexical_bindings: HashMap<String, BTreeSet<String>>,
}

impl TypedBodyIndex {
    pub fn function_origin(&self, key: &str) -> Option<SourceLocation> {
        self.origins.get(key).copied()
    }

    pub fn staged_constant(&self, key: &str) -> Option<&Expr> {
        self.constants.get(key)
    }

    pub fn staged_test(&self, key: &str) -> Option<&[Statement]> {
        self.tests.get(key).map(Vec::as_slice)
    }
}

pub fn lower_typed_bodies<'a>(
    modules: impl IntoIterator<Item = TypedModuleInput<'a>>,
    signatures: &TypedSignatureIndex,
) -> Result<TypedBodyIndex> {
    let modules: Vec<_> = modules.into_iter().collect();
    let declared_aliases = signatures.aliases.keys().cloned().collect();
    let mut bodies = TypedBodyIndex::default();
    for input in modules {
        for form in &input.module.forms {
            match form {
                TopLevel::Function(function) => lower_function(
                    input.alias,
                    input.module,
                    function,
                    &qualify(input.alias, &function.name.value),
                    signatures,
                    &declared_aliases,
                    &mut bodies,
                )?,
                TopLevel::Definition(definition) => {
                    for annotation in &definition.annotations {
                        if let AnnotationKind::Definitions(functions) = &annotation.value {
                            for function in functions {
                                let key = format!(
                                    "{}.{}",
                                    qualify(input.alias, &definition.name.value),
                                    function.name.value
                                );
                                lower_function(
                                    input.alias,
                                    input.module,
                                    function,
                                    &key,
                                    signatures,
                                    &declared_aliases,
                                    &mut bodies,
                                )?;
                            }
                        }
                    }
                }
                TopLevel::Constant(constant) => {
                    let key = qualify(input.alias, &constant.name.value);
                    let value = lower_expr(
                        input.alias,
                        &constant.value,
                        signatures,
                        &declared_aliases,
                        &BTreeSet::new(),
                    )
                    .with_context(|| format!("lowering typed constant `{key}`"))?;
                    if bodies.constants.insert(key.clone(), value).is_some() {
                        bail!("duplicate typed constant body `{key}`");
                    }
                }
                TopLevel::Test(test) => {
                    let key = qualify(input.alias, &test.name.value);
                    let statements = lower_statements(
                        input.alias,
                        &test.body,
                        signatures,
                        &declared_aliases,
                        &BTreeSet::new(),
                    )
                    .with_context(|| format!("lowering typed test `{key}`"))?;
                    if bodies.tests.insert(key.clone(), statements).is_some() {
                        bail!("duplicate typed test body `{key}`");
                    }
                }
                TopLevel::Import(_) | TopLevel::Macro(_) => {}
            }
        }
    }
    Ok(bodies)
}

/// Backward-compatible entry point for the checked typed executable subset.
pub fn materialize_typed_identity_functions(
    signatures: &TypedSignatureIndex,
    bodies: &TypedBodyIndex,
) -> Result<HashMap<String, FunctionSig>> {
    materialize_typed_functions(signatures, bodies)
}

/// Validate and materialize the safe, non-generic typed body subset.
///
/// Validation operates solely on semantic IR, so the same inference and
/// control-flow invariants are reusable by every source frontend.
pub fn materialize_typed_functions(
    signatures: &TypedSignatureIndex,
    bodies: &TypedBodyIndex,
) -> Result<HashMap<String, FunctionSig>> {
    let signature_keys: BTreeSet<_> = signatures.functions.keys().cloned().collect();
    let body_keys: BTreeSet<_> = bodies.functions.keys().cloned().collect();
    if signature_keys != body_keys {
        bail!(
            "typed signature/body set mismatch: signatures={signature_keys:?}, bodies={body_keys:?}"
        );
    }
    let constants = materialize_constants(signatures, bodies)?;
    bodies
        .functions
        .iter()
        .map(|(key, body)| {
            let signature = signatures
                .functions
                .get(key)
                .with_context(|| format!("typed body `{key}` has no signature"))?;
            let location = bodies
                .origins
                .get(key)
                .map(format_location)
                .unwrap_or_else(|| "at unknown source".to_string());
            if !signature.type_params.is_empty() {
                bail!(
                    "typed executable subset does not support generic function `{key}` {location}"
                );
            }
            for ty in signature.arg_types.iter().chain([&signature.return_type]) {
                ensure_safe_type(ty, key, &location)?;
            }
            let checked = match body {
                FunctionBody::Wasm { import, wasm_args } => {
                    // No `Statement`/`Expr` IR nodes are produced for a wasm
                    // import body, so no origins were recorded for it either
                    // (see `lower_function`); confirm that invariant instead
                    // of walking a cursor that would never advance.
                    let node_origins = bodies
                        .node_origins
                        .get(key)
                        .with_context(|| format!("typed body `{key}` has no node origins"))?;
                    if !node_origins.is_empty() {
                        bail!(
                            "typed wasm import body `{key}` unexpectedly recorded {} node origins",
                            node_origins.len()
                        );
                    }
                    validate_wasm_import_body(
                        key,
                        signature,
                        import,
                        wasm_args,
                        &signatures.aliases,
                        &location,
                    )?;
                    FunctionBody::Wasm {
                        import: import.clone(),
                        wasm_args: wasm_args.clone(),
                    }
                }
                FunctionBody::User { statements } => {
                    let mut locals: HashMap<_, _> = signature
                        .arg_names
                        .iter()
                        .cloned()
                        .zip(signature.arg_types.iter().cloned())
                        .collect();
                    let mut origins = OriginCursor::new(
                        bodies
                            .node_origins
                            .get(key)
                            .with_context(|| format!("typed body `{key}` has no node origins"))?,
                    );
                    let checked = validate_statements(
                        statements,
                        &mut locals,
                        &constants,
                        signatures,
                        bodies.let_types.get(key).unwrap_or(&HashMap::new()),
                        &signature.return_type,
                        0,
                        false,
                        key,
                        &mut origins,
                    )
                    .map_err(|error| origins.annotate(error))
                    .with_context(|| format!("validating typed function `{key}`"))?;
                    origins.finish()?;
                    validate_function_body(&checked, &signature.return_type)
                        .with_context(|| format!("validating returns in typed function `{key}`"))?;
                    validate_task_handles(&checked).with_context(|| {
                        format!("validating task handles in typed function `{key}`")
                    })?;
                    FunctionBody::User {
                        statements: checked,
                    }
                }
            };
            Ok((key.clone(), materialize(signature, checked)))
        })
        .collect()
}

/// Validate a `$wasm`-only typed body against the versioned host ABI
/// registry. Delegates to `crate::lower::validate_wasm_bodies`, the exact
/// legacy YAML-path check (`E-WASM-002`/`E-WASM-003`/`E-WASM-004`/
/// `E-WASM-007`/`E-CAP-002`), against a single-entry signature map built for
/// this function alone -- this is a trust boundary (host imports call out to
/// the runtime), so it must reuse the strict legacy rule rather than a
/// best-effort typed re-derivation.
fn validate_wasm_import_body(
    key: &str,
    signature: &TypedFunctionSignature,
    import: &ImportTarget,
    wasm_args: &[WasmArgSpec],
    aliases: &HashMap<String, TypeAlias>,
    location: &str,
) -> Result<()> {
    let mut sigs = HashMap::new();
    sigs.insert(
        key.to_string(),
        materialize(
            signature,
            FunctionBody::Wasm {
                import: import.clone(),
                wasm_args: wasm_args.to_vec(),
            },
        ),
    );
    crate::lower::validate_wasm_bodies(&sigs, aliases)
        .with_context(|| format!("validating typed wasm import `{key}` {location}"))
}

fn materialize_constants(
    signatures: &TypedSignatureIndex,
    bodies: &TypedBodyIndex,
) -> Result<HashMap<String, RuntimeValue>> {
    let mut constants = HashMap::new();
    for (key, expected) in &signatures.constants {
        ensure_safe_type(expected, key, "in typed constant")?;
        let Some(Expr::Value(value)) = bodies.constants.get(key) else {
            bail!("typed executable subset requires constant `{key}` to be a literal");
        };
        let actual = infer_expr_type(
            &Expr::Value(value.clone()),
            &constants,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        )
        .with_context(|| format!("cannot infer typed constant `{key}`"))?;
        if &actual != expected {
            bail!(
                "typed constant `{key}` declares {expected:?}, but its literal has type {actual:?}"
            );
        }
        constants.insert(key.clone(), value.clone());
    }
    Ok(constants)
}

fn ensure_safe_type(ty: &TypeRef, context: &str, location: &str) -> Result<()> {
    match ty {
        TypeRef::Bool
        | TypeRef::Str
        | TypeRef::Int8
        | TypeRef::Int16
        | TypeRef::Int32
        | TypeRef::Int64
        | TypeRef::UInt8
        | TypeRef::UInt16
        | TypeRef::UInt32
        | TypeRef::UInt64
        | TypeRef::Float32
        | TypeRef::Float64
        | TypeRef::Void
        | TypeRef::Range => Ok(()),
        TypeRef::Mutable(inner)
        | TypeRef::Array(inner)
        | TypeRef::Reference { inner, .. } => ensure_safe_type(inner, context, location),
        TypeRef::Record(fields) => fields
            .values()
            .try_for_each(|ty| ensure_safe_type(ty, context, location)),
        TypeRef::Tuple(items) => items
            .iter()
            .try_for_each(|ty| ensure_safe_type(ty, context, location)),
        TypeRef::Map { key, value } => {
            ensure_safe_type(key, context, location)?;
            ensure_safe_type(value, context, location)
        }
        other => bail!(
            "typed executable subset does not yet support {other:?} in `{context}` {location}; generics, enums, interfaces, nominal types, capabilities, and function types remain staged"
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_statements(
    statements: &[Statement],
    locals: &mut HashMap<String, TypeRef>,
    constants: &HashMap<String, RuntimeValue>,
    signatures: &TypedSignatureIndex,
    let_types: &HashMap<String, TypeRef>,
    return_type: &TypeRef,
    loop_depth: usize,
    in_task: bool,
    context: &str,
    origins: &mut OriginCursor<'_>,
) -> Result<Vec<Statement>> {
    let mut checked = Vec::with_capacity(statements.len());
    for statement in statements {
        let statement_origin = origins.enter()?;
        let statement = match statement {
            Statement::Call(call) => Statement::Call(validate_call(
                call,
                locals,
                constants,
                signatures,
                context,
                origins,
                &statement_origin,
            )?),
            Statement::Let { var, value } => {
                if locals.contains_key(var) {
                    bail!("typed local `{var}` is already bound in `{context}`");
                }
                let (value, ty) = match value {
                    LetValue::Expr(expr) => {
                        let expr =
                            validate_expr(expr, locals, constants, signatures, context, origins)?;
                        let ty = infer(&expr, locals, constants, signatures, context)?;
                        (LetValue::Expr(expr), ty)
                    }
                    LetValue::Call(call) => {
                        let call_origin = origins.enter()?;
                        let call = validate_call(
                            call,
                            locals,
                            constants,
                            signatures,
                            context,
                            origins,
                            &call_origin,
                        )?;
                        let ty = signatures.functions[&call.callee_key].return_type.clone();
                        (LetValue::Call(call), ty)
                    }
                };
                if ty == TypeRef::Void {
                    bail!("typed local `{var}` cannot bind a void value in `{context}`");
                }
                if let Some(expected) = let_types.get(var) {
                    if expected != &ty {
                        bail!(
                            "typed local `{var}` declares {expected:?}, but its value has type {ty:?}"
                        );
                    }
                }
                locals.insert(var.clone(), ty);
                Statement::Let {
                    var: var.clone(),
                    value,
                }
            }
            Statement::Set { var, value } => {
                let target = locals
                    .get(var)
                    .with_context(|| format!("E-SET-002: unknown typed set target `{var}`"))?;
                let writable = match target {
                    TypeRef::Mutable(inner)
                    | TypeRef::Reference {
                        inner,
                        mutable: true,
                    } => inner.as_ref(),
                    _ => bail!("E-SET-002: typed symbol `{var}` is not writable"),
                };
                let value = validate_expr(value, locals, constants, signatures, context, origins)?;
                let actual = infer(&value, locals, constants, signatures, context)?;
                if writable != &actual {
                    bail!(
                        "E-SET-003: typed assignment to `{var}` expects {writable:?}, got {actual:?}"
                    );
                }
                Statement::Set {
                    var: runtime_name(var, context, signatures),
                    value,
                }
            }
            Statement::Return(expr) => {
                if in_task {
                    bail!("typed task bodies cannot return from their enclosing function");
                }
                let expr = validate_expr(expr, locals, constants, signatures, context, origins)?;
                let actual = infer(&expr, locals, constants, signatures, context)?;
                if &actual != return_type {
                    bail!("typed return in `{context}` expects {return_type:?}, got {actual:?}");
                }
                Statement::Return(expr)
            }
            Statement::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond = validate_expr(cond, locals, constants, signatures, context, origins)?;
                if infer(&cond, locals, constants, signatures, context)? != TypeRef::Bool {
                    bail!("typed if condition in `{context}` must be bool");
                }
                let then_body = validate_statements(
                    then_body,
                    &mut locals.clone(),
                    constants,
                    signatures,
                    let_types,
                    return_type,
                    loop_depth,
                    in_task,
                    context,
                    origins,
                )?;
                let else_body = validate_statements(
                    else_body,
                    &mut locals.clone(),
                    constants,
                    signatures,
                    let_types,
                    return_type,
                    loop_depth,
                    in_task,
                    context,
                    origins,
                )?;
                Statement::If {
                    cond,
                    then_body,
                    else_body,
                }
            }
            Statement::While { cond, body } => {
                let cond = validate_expr(cond, locals, constants, signatures, context, origins)?;
                if infer(&cond, locals, constants, signatures, context)? != TypeRef::Bool {
                    bail!("typed while condition in `{context}` must be bool");
                }
                let body = validate_statements(
                    body,
                    &mut locals.clone(),
                    constants,
                    signatures,
                    let_types,
                    return_type,
                    loop_depth + 1,
                    in_task,
                    context,
                    origins,
                )?;
                Statement::While { cond, body }
            }
            Statement::For { var, source, body } => {
                let source =
                    validate_expr(source, locals, constants, signatures, context, origins)?;
                let item = match infer(&source, locals, constants, signatures, context)? {
                    TypeRef::Array(item) => *item,
                    TypeRef::Range => TypeRef::Int64,
                    other => bail!("typed for source must be array or range, got {other:?}"),
                };
                let mut nested = locals.clone();
                if nested.contains_key(var) {
                    origins.select(&statement_origin);
                    bail!("typed for binding `{var}` shadows an existing local");
                }
                nested.insert(var.clone(), item);
                let body = validate_statements(
                    body,
                    &mut nested,
                    constants,
                    signatures,
                    let_types,
                    return_type,
                    loop_depth + 1,
                    in_task,
                    context,
                    origins,
                )?;
                Statement::For {
                    var: var.clone(),
                    source,
                    body,
                }
            }
            Statement::Task { captures, body } => {
                let mut captured = HashMap::new();
                for capture in captures {
                    let ty = locals
                        .get(capture)
                        .with_context(|| format!("unknown typed task capture `{capture}`"))?;
                    if matches!(
                        ty,
                        TypeRef::Mutable(_) | TypeRef::Reference { mutable: true, .. }
                    ) {
                        bail!("typed task capture `{capture}` must be immutable");
                    }
                    captured.insert(capture.clone(), ty.clone());
                }
                let body = validate_statements(
                    body,
                    &mut captured,
                    constants,
                    signatures,
                    let_types,
                    &TypeRef::Void,
                    0,
                    true,
                    context,
                    origins,
                )?;
                let runtime_captures = captures
                    .iter()
                    .map(|capture| runtime_name(capture, context, signatures))
                    .collect();
                Statement::Task {
                    captures: runtime_captures,
                    body,
                }
            }
            Statement::Break | Statement::Continue => {
                if loop_depth == 0 {
                    bail!("typed loop control is only valid inside a loop");
                }
                statement.clone()
            }
            Statement::Eval(expr) => Statement::Eval(validate_expr(
                expr, locals, constants, signatures, context, origins,
            )?),
            Statement::Match { .. } => {
                bail!("typed match remains staged with enum and interface semantics")
            }
            Statement::Spawn {
                handle,
                captures,
                value,
                ..
            } => {
                if locals.contains_key(handle) {
                    bail!(
                        "E-TASK-003: typed task handle `{handle}` would shadow an existing local"
                    );
                }
                let mut task_locals = HashMap::new();
                for capture in captures {
                    let ty = locals.get(capture).with_context(|| {
                        format!("E-TASK-001: unknown typed task capture `{capture}`")
                    })?;
                    if matches!(
                        ty,
                        TypeRef::Mutable(_) | TypeRef::Reference { .. } | TypeRef::JoinHandle(_)
                    ) {
                        bail!(
                            "E-TASK-001: typed task capture `{capture}` has mutable, reference, \
                             or affine handle type {ty:?}; move an immutable snapshot into the \
                             task instead"
                        );
                    }
                    task_locals.insert(capture.clone(), ty.clone());
                }
                let value =
                    validate_expr(value, &task_locals, constants, signatures, context, origins)?;
                let result_type = infer(&value, &task_locals, constants, signatures, context)?;
                if result_type == TypeRef::Void {
                    bail!("E-TASK-002: typed `spawn` value must produce a non-void result");
                }
                locals.insert(
                    handle.clone(),
                    TypeRef::JoinHandle(Box::new(result_type.clone())),
                );
                let runtime_captures = captures
                    .iter()
                    .map(|capture| runtime_name(capture, context, signatures))
                    .collect();
                Statement::Spawn {
                    handle: handle.clone(),
                    captures: runtime_captures,
                    value,
                    result_type,
                }
            }
            Statement::Join { handle, var } => {
                if locals.contains_key(var) {
                    bail!("E-TASK-003: typed join result `{var}` would shadow an existing local");
                }
                let handle_type = locals.remove(handle).with_context(|| {
                    format!(
                        "E-TASK-003: typed task handle `{handle}` is unknown or was already joined"
                    )
                })?;
                let TypeRef::JoinHandle(result_type) = handle_type else {
                    bail!("E-TASK-003: typed symbol `{handle}` is not a task join handle");
                };
                locals.insert(var.clone(), *result_type);
                Statement::Join {
                    handle: handle.clone(),
                    var: var.clone(),
                }
            }
        };
        checked.push(statement);
    }
    Ok(checked)
}

fn validate_call(
    call: &Call,
    locals: &HashMap<String, TypeRef>,
    constants: &HashMap<String, RuntimeValue>,
    signatures: &TypedSignatureIndex,
    context: &str,
    origins: &mut OriginCursor<'_>,
    call_origin: &Origin,
) -> Result<Call> {
    origins.select(call_origin);
    if !call.type_args.is_empty() {
        bail!("typed generic calls remain staged");
    }
    let signature = signatures
        .functions
        .get(&call.callee_key)
        .with_context(|| format!("unknown typed call `{}`", call.callee_key))?;
    let caller = &signatures.functions[context];
    if signature.alias != caller.alias {
        let import_alias = call
            .callee_key
            .split('.')
            .next()
            .unwrap_or(&call.callee_key);
        let imported = signatures
            .imports
            .keys()
            .any(|(module, alias)| module.alias == caller.alias && alias == import_alias);
        if !imported {
            bail!(
                "typed call `{}` is outside the import scope of `{}`",
                call.callee_key,
                caller.alias
            );
        }
        if signatures.visibility.get(&call.callee_key) != Some(&crate::ast::Visibility::Public) {
            bail!("typed call `{}` is not publicly visible", call.callee_key);
        }
    }
    if !signature.type_params.is_empty() {
        bail!(
            "typed call `{}` targets a generic function",
            call.callee_key
        );
    }
    if call.args.len() != signature.arg_types.len() {
        bail!(
            "typed call `{}` expects {} arguments, got {}",
            call.callee_key,
            signature.arg_types.len(),
            call.args.len()
        );
    }
    let args = call
        .args
        .iter()
        .zip(&signature.arg_types)
        .map(|(argument, expected)| {
            let argument =
                validate_expr(argument, locals, constants, signatures, context, origins)?;
            let actual = infer(&argument, locals, constants, signatures, context)?;
            if &actual != expected {
                bail!(
                    "typed call `{}` expects {expected:?}, got {actual:?}",
                    call.callee_key
                );
            }
            Ok(argument)
        })
        .collect::<Result<_>>()?;
    Ok(Call {
        callee_key: call.callee_key.clone(),
        type_args: Vec::new(),
        args,
    })
}

fn validate_expr(
    expr: &Expr,
    locals: &HashMap<String, TypeRef>,
    constants: &HashMap<String, RuntimeValue>,
    signatures: &TypedSignatureIndex,
    context: &str,
    origins: &mut OriginCursor<'_>,
) -> Result<Expr> {
    let expression_origin = origins.enter()?;
    let validated = match expr {
        Expr::Value(_) => expr.clone(),
        Expr::VarRef(name) => {
            if locals.contains_key(name) {
                Expr::VarRef(runtime_name(name, context, signatures))
            } else {
                let caller = &signatures.functions[context];
                let local = qualify(&caller.alias, name);
                let resolved = if constants.contains_key(&local) {
                    local.clone()
                } else if constants.contains_key(name) {
                    name.clone()
                } else {
                    bail!("unknown typed reference `{name}` in `{context}`");
                };
                let is_local = if caller.alias.is_empty() {
                    !name.contains('.')
                } else {
                    resolved.starts_with(&format!("{}.", caller.alias))
                };
                if !is_local {
                    let import_alias = resolved.split('.').next().unwrap_or(&resolved);
                    let imported = signatures.imports.keys().any(|(module, alias)| {
                        module.alias == caller.alias && alias == import_alias
                    });
                    if !imported
                        || signatures.visibility.get(&resolved)
                            != Some(&crate::ast::Visibility::Public)
                    {
                        bail!(
                            "typed constant `{resolved}` is outside scope or not publicly visible"
                        );
                    }
                }
                Expr::Value(constants[&resolved].clone())
            }
        }
        Expr::Call { call, return_type } => Expr::Call {
            call: Box::new(validate_call(
                call,
                locals,
                constants,
                signatures,
                context,
                origins,
                &expression_origin,
            )?),
            return_type: return_type.clone(),
        },
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(name, value)| {
                    Ok((
                        name.clone(),
                        validate_expr(value, locals, constants, signatures, context, origins)?,
                    ))
                })
                .collect::<Result<_>>()?,
        ),
        Expr::Tuple(items) => Expr::Tuple(
            items
                .iter()
                .map(|item| validate_expr(item, locals, constants, signatures, context, origins))
                .collect::<Result<_>>()?,
        ),
        Expr::Array(items) => Expr::Array(
            items
                .iter()
                .map(|item| validate_expr(item, locals, constants, signatures, context, origins))
                .collect::<Result<_>>()?,
        ),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    Ok((
                        validate_expr(key, locals, constants, signatures, context, origins)?,
                        validate_expr(value, locals, constants, signatures, context, origins)?,
                    ))
                })
                .collect::<Result<_>>()?,
        ),
        Expr::Mutable(inner) => Expr::Mutable(Box::new(validate_expr(
            inner, locals, constants, signatures, context, origins,
        )?)),
        Expr::Reference { target, mutable } => Expr::Reference {
            target: Box::new(validate_expr(
                target, locals, constants, signatures, context, origins,
            )?),
            mutable: *mutable,
        },
        Expr::Range { start, end, step } => {
            let start = validate_expr(start, locals, constants, signatures, context, origins)?;
            let start_origin = origins.selected().cloned();
            let end = validate_expr(end, locals, constants, signatures, context, origins)?;
            let end_origin = origins.selected().cloned();
            let step = validate_expr(step, locals, constants, signatures, context, origins)?;
            let step_origin = origins.selected().cloned();
            for (value, origin) in [
                (&start, start_origin.as_ref()),
                (&end, end_origin.as_ref()),
                (&step, step_origin.as_ref()),
            ] {
                if let Some(origin) = origin {
                    origins.select(origin);
                }
                if infer(value, locals, constants, signatures, context)? != TypeRef::Int64 {
                    bail!("typed range bounds in `{context}` must be int64");
                }
            }
            Expr::Range {
                start: Box::new(start),
                end: Box::new(end),
                step: Box::new(step),
            }
        }
        Expr::Primitive { op, args, .. } => {
            let args: Vec<Expr> = args
                .iter()
                .map(|arg| validate_expr(arg, locals, constants, signatures, context, origins))
                .collect::<Result<_>>()?;
            let types = args
                .iter()
                .map(|arg| infer(arg, locals, constants, signatures, context))
                .collect::<Result<Vec<_>>>()?;
            let operand_type = types[0].clone();
            if types.iter().any(|ty| ty != &operand_type) {
                bail!(
                    "E-OP-001: `{}` operands must have the same primitive type; explicit `cast` is required",
                    typed_primitive_name(*op)
                );
            }
            if !typed_primitive_valid_for(*op, &operand_type) {
                bail!(
                    "E-OP-001: `{}` is not defined for {operand_type:?}",
                    typed_primitive_name(*op)
                );
            }
            let return_type = typed_primitive_return_type(*op, &operand_type);
            Expr::Primitive {
                op: *op,
                args,
                operand_type,
                return_type,
            }
        }
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let payload = payload
                .as_ref()
                .map(|payload| {
                    validate_expr(payload, locals, constants, signatures, context, origins)
                })
                .transpose()?
                .map(Box::new);
            let alias = signatures
                .aliases
                .get(enum_key)
                .with_context(|| format!("unknown typed enum `{enum_key}`"))?;
            let TypeRef::Enum(tags) = &alias.body else {
                bail!("typed constructor target `{enum_key}` is not an enum");
            };
            let payload_ty = tags
                .get(tag)
                .with_context(|| format!("unknown typed enum tag `{tag}` for enum `{enum_key}`"))?;
            match (&payload, payload_ty) {
                (None, ty) if ty == &TypeRef::Void => {}
                (Some(payload), ty) if ty != &TypeRef::Void => {
                    let actual = infer(payload, locals, constants, signatures, context)?;
                    if !type_compatible(ty, &actual, &signatures.aliases) {
                        bail!(
                            "constructor `{enum_key}.{tag}` payload type mismatch: expected {ty:?}, got {actual:?}"
                        );
                    }
                }
                _ => bail!(
                    "constructor `{enum_key}.{tag}` payload arity does not match its declared tag"
                ),
            }
            Expr::EnumConstructor {
                enum_key: enum_key.clone(),
                tag: tag.clone(),
                payload,
            }
        }
        Expr::Cast { .. } => bail!("typed cast forms remain staged"),
        Expr::PolicyNarrow { from, target } => {
            let from = validate_expr(from, locals, constants, signatures, context, origins)?;
            let source = infer(&from, locals, constants, signatures, context)?;
            if !policy_type_is_subset(&source, target, &signatures.aliases) {
                bail!("policy narrowing cannot widen authority");
            }
            Expr::PolicyNarrow {
                from: Box::new(from),
                target: target.clone(),
            }
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            let cond = validate_expr(cond, locals, constants, signatures, context, origins)?;
            if infer(&cond, locals, constants, signatures, context)? != TypeRef::Bool {
                bail!("typed expression-`if` condition in `{context}` must be bool");
            }
            let then_e = validate_expr(then_e, locals, constants, signatures, context, origins)?;
            let else_e = validate_expr(else_e, locals, constants, signatures, context, origins)?;
            let then_ty = infer(&then_e, locals, constants, signatures, context)?;
            let else_ty = infer(&else_e, locals, constants, signatures, context)?;
            if !type_compatible(&then_ty, &else_ty, &signatures.aliases)
                && !type_compatible(&else_ty, &then_ty, &signatures.aliases)
            {
                bail!(
                    "typed expression-`if` branches in `{context}` have incompatible types {then_ty:?} and {else_ty:?}"
                );
            }
            Expr::If {
                cond: Box::new(cond),
                then_e: Box::new(then_e),
                else_e: Box::new(else_e),
            }
        }
    };
    origins.select(&expression_origin);
    Ok(validated)
}

fn infer(
    expr: &Expr,
    locals: &HashMap<String, TypeRef>,
    constants: &HashMap<String, RuntimeValue>,
    signatures: &TypedSignatureIndex,
    context: &str,
) -> Result<TypeRef> {
    let mut inference_locals = locals.clone();
    inference_locals.extend(
        locals
            .iter()
            .map(|(name, ty)| (format!("args.{name}"), ty.clone())),
    );
    let enums = typed_enum_defs(signatures);
    infer_expr_type(
        expr,
        constants,
        &inference_locals,
        &signatures.aliases,
        &enums,
    )
    .with_context(|| format!("cannot infer typed expression in `{context}`"))
}

/// Build a legacy-shaped `EnumDef` registry from the typed alias table, so
/// the shared `infer_expr_type` (which predates typed aliases) can resolve
/// `Expr::EnumConstructor` payload and instantiated-type inference exactly as
/// the legacy YAML path does.
fn typed_enum_defs(signatures: &TypedSignatureIndex) -> HashMap<String, EnumDef> {
    signatures
        .aliases
        .iter()
        .filter_map(|(key, alias)| match &alias.body {
            TypeRef::Enum(tags) => Some((
                key.clone(),
                EnumDef {
                    alias: alias.alias.clone(),
                    name: alias.name.clone(),
                    type_params: alias.type_params.clone(),
                    type_param_bounds: alias.type_param_bounds.clone(),
                    tags: tags.clone(),
                },
            )),
            _ => None,
        })
        .collect()
}

fn format_location(location: &SourceLocation) -> String {
    format!(
        "at document {} bytes {}..{}",
        location.document.raw(),
        location.span.start,
        location.span.end
    )
}

fn materialize(signature: &TypedFunctionSignature, body: FunctionBody) -> FunctionSig {
    FunctionSig {
        alias: signature.alias.clone(),
        symbol: signature.symbol.clone(),
        type_params: signature.type_params.clone(),
        type_param_bounds: signature.type_param_bounds.clone(),
        arg_names: signature.arg_names.clone(),
        arg_types: signature.arg_types.clone(),
        return_type: signature.return_type.clone(),
        body,
        doc: signature.doc.clone(),
    }
}

fn lower_function(
    module_alias: &str,
    module: &Module,
    function: &Function,
    key: &str,
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    bodies: &mut TypedBodyIndex,
) -> Result<()> {
    // A `$wasm`-only body is a distinct function *body kind* (like legacy
    // `is_wasm_only_body`/`extract_wasm_body`), not an ordinary statement
    // sequence: it commits only when the entire body is a single `(wasm
    // import: ... args: ...)` form. It produces no `Statement`/`Expr` IR, so
    // it skips `lower_statements`, `collect_body_metadata`, and origin
    // collection entirely -- there is nothing for those passes to walk.
    if let [AstExpr {
        value: ExprKind::Wasm { import, arguments },
        ..
    }] = function.body.as_slice()
    {
        let wasm_import = ImportTarget {
            module: import.module.value.clone(),
            name: import.name.value.clone(),
        };
        let wasm_args = arguments.iter().map(lower_wasm_argument).collect();
        if bodies
            .functions
            .insert(
                key.to_string(),
                FunctionBody::Wasm {
                    import: wasm_import,
                    wasm_args,
                },
            )
            .is_some()
        {
            bail!("duplicate typed function body `{key}`");
        }
        let location = SourceLocation {
            document: function.origin.document_id().unwrap_or(module.document_id),
            span: function.origin.primary_span(),
        };
        bodies.origins.insert(key.to_string(), location);
        bodies.let_types.insert(key.to_string(), HashMap::new());
        bodies
            .lexical_bindings
            .insert(key.to_string(), BTreeSet::new());
        bodies.node_origins.insert(key.to_string(), Vec::new());
        return Ok(());
    }
    let generic_names = signatures
        .functions
        .get(key)
        .with_context(|| format!("typed function `{key}` has no signature"))?
        .type_params
        .iter()
        .cloned()
        .collect();
    let statements = lower_statements(
        module_alias,
        &function.body,
        signatures,
        declared_aliases,
        &generic_names,
    )
    .with_context(|| format!("lowering typed function `{key}`"))?;
    let mut let_types = HashMap::new();
    let mut lexical_bindings = BTreeSet::new();
    collect_body_metadata(
        &function.body,
        module_alias,
        declared_aliases,
        &generic_names,
        &mut let_types,
        &mut lexical_bindings,
    )?;
    let mut node_origins = Vec::new();
    collect_statement_origins(&function.body, module_alias, signatures, &mut node_origins);
    if bodies
        .functions
        .insert(key.to_string(), FunctionBody::User { statements })
        .is_some()
    {
        bail!("duplicate typed function body `{key}`");
    }
    let location = SourceLocation {
        document: function.origin.document_id().unwrap_or(module.document_id),
        span: function.origin.primary_span(),
    };
    bodies.origins.insert(key.to_string(), location);
    bodies.let_types.insert(key.to_string(), let_types);
    bodies
        .lexical_bindings
        .insert(key.to_string(), lexical_bindings);
    bodies.node_origins.insert(key.to_string(), node_origins);
    Ok(())
}

/// Lower one `$wasm.args` forwarding spec. `WasmArgument::Parameter` names a
/// function argument by its bare name (the same convention every other
/// argument reference in the typed surface uses, e.g. `self` not
/// `args.self`); legacy `parse_wasm_arg_spec` strips its own `$args.`
/// marker down to the same bare name, so both paths agree on
/// `WasmArgSpec::Arg` holding the caller's plain argument name.
fn lower_wasm_argument(argument: &WasmArgument) -> WasmArgSpec {
    match argument {
        WasmArgument::Parameter(name) => WasmArgSpec::Arg(name.value.clone()),
        WasmArgument::ConstInt(value) => WasmArgSpec::ConstInt(value.value),
        WasmArgument::ConstString(value) => WasmArgSpec::ConstStr(value.value.clone()),
    }
}

fn runtime_name(name: &str, context: &str, signatures: &TypedSignatureIndex) -> String {
    if signatures
        .functions
        .get(context)
        .is_some_and(|signature| signature.arg_names.iter().any(|argument| argument == name))
    {
        format!("args.{name}")
    } else {
        name.to_string()
    }
}

fn collect_body_metadata(
    expressions: &[AstExpr],
    module_alias: &str,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
    let_types: &mut HashMap<String, TypeRef>,
    bindings: &mut BTreeSet<String>,
) -> Result<()> {
    for expression in expressions {
        match &expression.value {
            ExprKind::Do(body) | ExprKind::While { body, .. } | ExprKind::Task { body, .. } => {
                collect_body_metadata(
                    body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                )?
            }
            ExprKind::If {
                then_body,
                else_body,
                ..
            } => {
                collect_body_metadata(
                    then_body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                )?;
                collect_body_metadata(
                    else_body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                )?;
            }
            ExprKind::Match { cases, .. } => {
                for case in cases {
                    collect_body_metadata(
                        &case.body,
                        module_alias,
                        declared_aliases,
                        generics,
                        let_types,
                        bindings,
                    )?;
                }
            }
            ExprKind::Let { name, ty, .. } => {
                if !bindings.insert(name.value.clone()) {
                    bail!(
                        "typed lexical binding `{}` is reused; body materialization requires globally unique local names",
                        name.value
                    );
                }
                if let Some(ty) = ty {
                    let ty = lower_type(ty, generics, module_alias, declared_aliases)?;
                    let_types.insert(name.value.clone(), ty);
                }
            }
            ExprKind::Return(Some(value))
            | ExprKind::Mutable(value)
            | ExprKind::ReferenceOf(value)
            | ExprKind::Cast { value, .. }
            | ExprKind::PolicyNarrow { value, .. }
            | ExprKind::Convert { value, .. } => collect_body_metadata(
                std::slice::from_ref(value.as_ref()),
                module_alias,
                declared_aliases,
                generics,
                let_types,
                bindings,
            )?,
            ExprKind::For { binding, body, .. } => {
                if !bindings.insert(binding.value.clone()) {
                    bail!(
                        "typed lexical binding `{}` is reused; body materialization requires globally unique local names",
                        binding.value
                    );
                }
                collect_body_metadata(
                    body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

struct OriginCursor<'a> {
    origins: &'a [Origin],
    next: usize,
    selected: Option<Origin>,
}

impl<'a> OriginCursor<'a> {
    fn new(origins: &'a [Origin]) -> Self {
        Self {
            origins,
            next: 0,
            selected: None,
        }
    }

    fn enter(&mut self) -> Result<Origin> {
        let origin = self.origins.get(self.next).cloned().with_context(|| {
            format!(
                "typed IR node {} has no corresponding source origin ({} available)",
                self.next,
                self.origins.len()
            )
        })?;
        self.next += 1;
        self.selected = Some(origin.clone());
        Ok(origin)
    }

    fn select(&mut self, origin: &Origin) {
        self.selected = Some(origin.clone());
    }

    fn selected(&self) -> Option<&Origin> {
        self.selected.as_ref()
    }

    fn annotate(&self, error: anyhow::Error) -> anyhow::Error {
        match &self.selected {
            Some(origin) => error.context(format_origin(origin)),
            None => error.context("at unknown typed IR node"),
        }
    }

    fn finish(&self) -> Result<()> {
        if self.next != self.origins.len() {
            bail!(
                "typed IR/source origin mismatch: consumed {} of {} node origins",
                self.next,
                self.origins.len()
            );
        }
        Ok(())
    }
}

fn collect_statement_origins(
    expressions: &[AstExpr],
    module_alias: &str,
    signatures: &TypedSignatureIndex,
    origins: &mut Vec<Origin>,
) {
    for expression in expressions {
        if let ExprKind::Do(items) = &expression.value {
            collect_statement_origins(items, module_alias, signatures, origins);
        } else {
            collect_statement_origin(expression, module_alias, signatures, origins);
        }
    }
}

fn collect_statement_origin(
    expression: &AstExpr,
    module_alias: &str,
    signatures: &TypedSignatureIndex,
    origins: &mut Vec<Origin>,
) {
    origins.push(expression.origin.clone());
    match &expression.value {
        ExprKind::Call { callee, arguments } => {
            if typed_primitive_head(&callee.value).is_some()
                || typed_enum_constructor_head(&callee.value, module_alias, signatures).is_some()
            {
                // Primitive and enum-constructor calls in statement position
                // lower to `Statement::Eval`, which validates the wrapped
                // expression through the generic `validate_expr` path and
                // therefore consumes one extra origin for the expression
                // node itself, matching the default arm below.
                origins.push(expression.origin.clone());
            }
            collect_expr_origins(arguments, origins)
        }
        ExprKind::Let { value, .. } | ExprKind::Set { value, .. } => {
            collect_expr_origin(value, origins)
        }
        ExprKind::Return(Some(value)) => collect_expr_origin(value, origins),
        // A source-level bare return lowers to a synthetic `unit` IR
        // expression. Give that child the statement origin so the one-to-one
        // IR mapping remains total.
        ExprKind::Return(None) => origins.push(expression.origin.clone()),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            collect_expr_origin(condition, origins);
            collect_statement_origins(then_body, module_alias, signatures, origins);
            collect_statement_origins(else_body, module_alias, signatures, origins);
        }
        ExprKind::While { condition, body } => {
            collect_expr_origin(condition, origins);
            collect_statement_origins(body, module_alias, signatures, origins);
        }
        ExprKind::For { source, body, .. } => {
            collect_expr_origin(source, origins);
            collect_statement_origins(body, module_alias, signatures, origins);
        }
        ExprKind::Match { target, cases } => {
            collect_expr_origin(target, origins);
            for case in cases {
                collect_statement_origins(&case.body, module_alias, signatures, origins);
            }
        }
        ExprKind::Task { body, .. } => {
            collect_statement_origins(body, module_alias, signatures, origins)
        }
        ExprKind::Spawn { value, .. } => collect_expr_origin(value, origins),
        ExprKind::Break | ExprKind::Continue | ExprKind::Join { .. } => {}
        _ => {
            // Expression statements lower to an `Eval` statement containing
            // a distinct expression IR node.
            origins.push(expression.origin.clone());
            collect_expr_children(expression, origins);
        }
    }
}

fn collect_expr_origins(expressions: &[AstExpr], origins: &mut Vec<Origin>) {
    for expression in expressions {
        collect_expr_origin(expression, origins);
    }
}

fn collect_expr_origin(expression: &AstExpr, origins: &mut Vec<Origin>) {
    origins.push(expression.origin.clone());
    collect_expr_children(expression, origins);
}

fn collect_expr_children(expression: &AstExpr, origins: &mut Vec<Origin>) {
    match &expression.value {
        ExprKind::Call { arguments, .. } => collect_expr_origins(arguments, origins),
        ExprKind::Record(fields) => {
            for field in fields {
                collect_expr_origin(&field.value, origins);
            }
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => collect_expr_origins(items, origins),
        ExprKind::Map(items) => {
            for (key, value) in items {
                collect_expr_origin(key, origins);
                collect_expr_origin(value, origins);
            }
        }
        ExprKind::Mutable(value)
        | ExprKind::ReferenceOf(value)
        | ExprKind::Cast { value, .. }
        | ExprKind::PolicyNarrow { value, .. } => collect_expr_origin(value, origins),
        ExprKind::Range(start, end, step) => {
            collect_expr_origin(start, origins);
            collect_expr_origin(end, origins);
            collect_expr_origin(step, origins);
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            collect_expr_origin(condition, origins);
            collect_if_branch_origin(&expression.origin, then_body, origins);
            collect_if_branch_origin(&expression.origin, else_body, origins);
        }
        ExprKind::Literal(_) | ExprKind::Reference(_) => {}
        _ => {}
    }
}

/// Origin bookkeeping for an expression-position `if` branch, which lowers
/// (`lower_if_branch`) to exactly one `Expr` node: the branch's single form
/// when present, or a synthetic `unit` value when the branch body is empty.
/// A branch with more than one form is rejected at lowering time, so no
/// origin is needed for that case (lowering never reaches the point where
/// `node_origins` is consulted).
fn collect_if_branch_origin(
    if_expression_origin: &Origin,
    body: &[AstExpr],
    origins: &mut Vec<Origin>,
) {
    match body {
        [] => origins.push(if_expression_origin.clone()),
        [single] => collect_expr_origin(single, origins),
        _ => {}
    }
}

fn format_origin(origin: &Origin) -> String {
    fn append(origin: &Origin, output: &mut String) {
        match origin {
            Origin::Source(span) => {
                output.push_str(&format!("source bytes {}..{}", span.start, span.end));
            }
            Origin::DocumentSource { document, span, .. } => {
                output.push_str(&format!(
                    "document {} bytes {}..{}",
                    document.raw(),
                    span.start,
                    span.end
                ));
            }
            Origin::Expansion {
                call_site,
                definition,
                parent,
            } => {
                output.push_str(&format!(
                    "macro call bytes {}..{}; macro definition bytes {}..{}; generated from ",
                    call_site.start, call_site.end, definition.start, definition.end
                ));
                append(parent, output);
            }
            Origin::DocumentExpansion {
                call_site,
                definition,
                parent,
                ..
            } => {
                output.push_str(&format!(
                    "macro call document {} bytes {}..{}; macro definition document {} bytes {}..{}; generated from ",
                    call_site.document.raw(),
                    call_site.span.start,
                    call_site.span.end,
                    definition.document.raw(),
                    definition.span.start,
                    definition.span.end
                ));
                append(parent, output);
            }
        }
    }

    let mut output = String::from("at ");
    append(origin, &mut output);
    output
}

fn lower_statements(
    module_alias: &str,
    expressions: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Vec<Statement>> {
    expressions
        .iter()
        .flat_map(|expression| match &expression.value {
            ExprKind::Do(items) => items.iter().collect(),
            _ => vec![expression],
        })
        .map(|expression| {
            lower_statement(
                module_alias,
                expression,
                signatures,
                declared_aliases,
                generics,
            )
        })
        .collect()
}

fn lower_statement(
    module_alias: &str,
    expression: &AstExpr,
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Statement> {
    let expr = |value| lower_expr(module_alias, value, signatures, declared_aliases, generics);
    let body = |values: &[AstExpr]| {
        lower_statements(module_alias, values, signatures, declared_aliases, generics)
    };
    Ok(match &expression.value {
        ExprKind::Call { callee, arguments } => match lower_typed_call(
            module_alias,
            &callee.value,
            arguments,
            signatures,
            declared_aliases,
            generics,
        )? {
            TypedCallResolution::Call(call) => Statement::Call(call),
            TypedCallResolution::Expr(expr) => Statement::Eval(expr),
        },
        ExprKind::Let { name, ty, value } => {
            if let Some(ty) = ty {
                let _ = lower_type(ty, generics, module_alias, declared_aliases)?;
            }
            let value = match &value.value {
                ExprKind::Call { callee, arguments } => match lower_typed_call(
                    module_alias,
                    &callee.value,
                    arguments,
                    signatures,
                    declared_aliases,
                    generics,
                )? {
                    TypedCallResolution::Call(call) => LetValue::Call(call),
                    TypedCallResolution::Expr(expr) => LetValue::Expr(expr),
                },
                _ => LetValue::Expr(expr(value)?),
            };
            Statement::Let {
                var: name.value.clone(),
                value,
            }
        }
        ExprKind::Set { name, value } => Statement::Set {
            var: name.value.clone(),
            value: expr(value)?,
        },
        ExprKind::Return(value) => Statement::Return(match value {
            Some(value) => expr(value)?,
            None => Expr::Value(RuntimeValue::Void),
        }),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => Statement::If {
            cond: expr(condition)?,
            then_body: body(then_body)?,
            else_body: body(else_body)?,
        },
        ExprKind::While {
            condition,
            body: loop_body,
        } => Statement::While {
            cond: expr(condition)?,
            body: body(loop_body)?,
        },
        ExprKind::For {
            binding,
            source,
            body: loop_body,
        } => Statement::For {
            var: binding.value.clone(),
            source: expr(source)?,
            body: body(loop_body)?,
        },
        ExprKind::Match { target, cases } => Statement::Match {
            target: expr(target)?,
            arms: cases
                .iter()
                .map(|case| {
                    Ok(MatchArm {
                        pattern: lower_pattern(
                            module_alias,
                            &case.pattern,
                            declared_aliases,
                            generics,
                        )?,
                        body: body(&case.body)?,
                    })
                })
                .collect::<Result<_>>()?,
        },
        ExprKind::Task {
            captures,
            body: task_body,
        } => Statement::Task {
            captures: captures.iter().map(|name| name.value.clone()).collect(),
            body: body(task_body)?,
        },
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => Statement::Spawn {
            handle: handle.value.clone(),
            captures: captures.iter().map(|name| name.value.clone()).collect(),
            value: expr(value)?,
            // Placeholder: replaced with the inferred result type by
            // `validate_statements`, which runs once locals are available.
            result_type: TypeRef::Void,
        },
        ExprKind::Join { handle, binding } => Statement::Join {
            handle: handle.value.clone(),
            var: binding.value.clone(),
        },
        ExprKind::Break => Statement::Break,
        ExprKind::Continue => Statement::Continue,
        ExprKind::Do(_) => bail!("nested `do` is only valid as a statement sequence"),
        _ => Statement::Eval(expr(expression)?),
    })
}

fn lower_expr(
    module_alias: &str,
    expression: &AstExpr,
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Expr> {
    let lower = |value| lower_expr(module_alias, value, signatures, declared_aliases, generics);
    Ok(match &expression.value {
        ExprKind::Literal(value) => Expr::Value(lower_literal(value)),
        ExprKind::Reference(name) => Expr::VarRef(name.clone()),
        ExprKind::Call { callee, arguments } => match lower_typed_call(
            module_alias,
            &callee.value,
            arguments,
            signatures,
            declared_aliases,
            generics,
        )? {
            TypedCallResolution::Expr(expr) => expr,
            TypedCallResolution::Call(call) => {
                let return_type = signatures
                    .functions
                    .get(&call.callee_key)
                    .with_context(|| {
                        format!(
                            "typed call resolved to callee `{}`, which has no signature",
                            call.callee_key
                        )
                    })?
                    .return_type
                    .clone();
                Expr::Call {
                    call: Box::new(call),
                    return_type,
                }
            }
        },
        ExprKind::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|field| Ok((field.name.value.clone(), lower(&field.value)?)))
                .collect::<Result<_>>()?,
        ),
        ExprKind::Tuple(items) => Expr::Tuple(items.iter().map(lower).collect::<Result<_>>()?),
        ExprKind::Array(items) => Expr::Array(items.iter().map(lower).collect::<Result<_>>()?),
        ExprKind::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| Ok((lower(key)?, lower(value)?)))
                .collect::<Result<_>>()?,
        ),
        ExprKind::Mutable(value) => Expr::Mutable(Box::new(lower(value)?)),
        ExprKind::ReferenceOf(value) => Expr::Reference {
            target: Box::new(lower(value)?),
            mutable: false,
        },
        ExprKind::Range(start, end, step) => Expr::Range {
            start: Box::new(lower(start)?),
            end: Box::new(lower(end)?),
            step: Box::new(lower(step)?),
        },
        ExprKind::Cast { value, into } => Expr::Cast {
            from: Box::new(lower(value)?),
            target: lower_type(into, generics, module_alias, declared_aliases)?,
        },
        ExprKind::PolicyNarrow { value, into } => {
            let target = lower_type(into, generics, module_alias, declared_aliases)?;
            let from = Box::new(lower(value)?);
            // `policy.narrow` is its own head, distinct from `cast`: a
            // capability/policy target needs subsumption checking
            // (`policy_type_is_subset`) instead of the newtype-cast rule
            // (`valid_cast_path`), so it must not be silently inferred from
            // the target type of an otherwise-ordinary `cast`. Normalize a
            // named alias down to the concrete `Capability`/`Policy` variant
            // here, matching the legacy `$policy.narrow` envelope.
            if let Some(capability) = capability_body(&target, &signatures.aliases).cloned() {
                Expr::PolicyNarrow {
                    from,
                    target: TypeRef::Capability(capability),
                }
            } else if let Some(policy) = policy_body(&target, &signatures.aliases).cloned() {
                Expr::PolicyNarrow {
                    from,
                    target: TypeRef::Policy(policy),
                }
            } else {
                bail!(
                    "typed `policy.narrow` target must be a capability or policy type, got {target:?}"
                );
            }
        }
        ExprKind::Convert { .. } => {
            bail!("typed `convert` lowering requires explicit fallback semantics")
        }
        ExprKind::Embed { .. } | ExprKind::Template { .. } => {
            bail!("typed compile-time expression lowering is not active")
        }
        // A `$wasm` form is a function *body kind*, not a general
        // expression: `lower_function` recognizes and lowers it only when
        // it is the function's sole body form. Reaching this generic
        // expression-position dispatch means it was nested inside a larger
        // body (e.g. alongside other statements, or inside an `if`), which
        // has no legacy equivalent and stays rejected explicitly rather
        // than silently accepted.
        ExprKind::Wasm { .. } => {
            bail!("typed `wasm` import body must be a function's entire body, not a nested expression")
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => Expr::If {
            cond: Box::new(lower(condition)?),
            then_e: Box::new(lower_if_branch(
                module_alias,
                then_body,
                signatures,
                declared_aliases,
                generics,
            )?),
            else_e: Box::new(lower_if_branch(
                module_alias,
                else_body,
                signatures,
                declared_aliases,
                generics,
            )?),
        },
        ExprKind::Do(_)
        | ExprKind::Let { .. }
        | ExprKind::Set { .. }
        | ExprKind::Return(_)
        | ExprKind::While { .. }
        | ExprKind::For { .. }
        | ExprKind::Match { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Task { .. }
        | ExprKind::Spawn { .. }
        | ExprKind::Join { .. } => {
            bail!("typed control form is not valid in expression position")
        }
    })
}

fn lower_call(
    module_alias: &str,
    callee: &str,
    arguments: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Call> {
    let local = qualify(module_alias, callee);
    let callee_key = if signatures.functions.contains_key(&local) {
        local
    } else if signatures.functions.contains_key(callee) {
        callee.to_string()
    } else {
        bail!("unknown typed callee `{callee}`");
    };
    Ok(Call {
        callee_key,
        type_args: Vec::new(),
        args: arguments
            .iter()
            .map(|argument| {
                lower_expr(
                    module_alias,
                    argument,
                    signatures,
                    declared_aliases,
                    generics,
                )
            })
            .collect::<Result<_>>()?,
    })
}

/// A call-shaped AST node resolves to either an ordinary user-function call,
/// or a value-producing IR expression (a primitive operator or an enum
/// constructor). The two are wrapped differently depending on where the call
/// appears (`Statement::Call`/`Statement::Eval`, `LetValue::Call`/
/// `LetValue::Expr`, or an inline `Expr::Call`/other `Expr`).
enum TypedCallResolution {
    Call(Call),
    Expr(Expr),
}

/// Resolve and lower a call-shaped AST node, checking (in order) whether its
/// head is a primitive operator, an enum constructor, or an ordinary
/// function call.
fn lower_typed_call(
    module_alias: &str,
    callee: &str,
    arguments: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<TypedCallResolution> {
    if let Some((op, arity)) = typed_primitive_head(callee) {
        return Ok(TypedCallResolution::Expr(lower_primitive_call(
            module_alias,
            callee,
            op,
            arity,
            arguments,
            signatures,
            declared_aliases,
            generics,
        )?));
    }
    if let Some((enum_key, tag)) = typed_enum_constructor_head(callee, module_alias, signatures) {
        return Ok(TypedCallResolution::Expr(lower_enum_constructor_call(
            module_alias,
            &enum_key,
            &tag,
            arguments,
            signatures,
            declared_aliases,
            generics,
        )?));
    }
    Ok(TypedCallResolution::Call(lower_call(
        module_alias,
        callee,
        arguments,
        signatures,
        declared_aliases,
        generics,
    )?))
}

/// Resolution rule: an unqualified call head matching one of the 22 typed
/// primitive names always resolves to the primitive, never to a user
/// function of the same name (declaring one is rejected at signature time in
/// `typed_lower.rs`). A qualified head (e.g. `mymod.add`) is never a
/// primitive, regardless of its suffix.
fn typed_primitive_head(callee: &str) -> Option<(PrimitiveOp, usize)> {
    if callee.contains('.') {
        None
    } else {
        typed_primitive_op(callee)
    }
}

/// Resolve a call head as an enum constructor reference: the surface grammar
/// has no dedicated constructor syntax, so `(mytype.tag ...)` is ambiguous
/// with an ordinary qualified call to a function named `tag` declared under
/// (or imported as) `mytype`. The stdlib itself has exactly this shape:
/// `option` is a real enum, but `option.empty` is an ordinary function, not
/// its `empty` tag (`option` only declares `some`/`none`).
///
/// Resolution therefore commits to the enum-constructor interpretation only
/// on a *full* match: the qualified prefix must name a registered enum type
/// alias, *and* the suffix must be one of that enum's declared tags. A
/// prefix match alone (enum exists, tag does not) must fall through to
/// ordinary call resolution rather than foreclosing it with a hard error --
/// otherwise a real function that merely shares a name with an enum module
/// can never be called.
fn typed_enum_constructor_head(
    callee: &str,
    module_alias: &str,
    signatures: &TypedSignatureIndex,
) -> Option<(String, String)> {
    let (enum_name, tag) = callee.rsplit_once('.')?;
    if enum_name.is_empty() || tag.is_empty() {
        return None;
    }
    let enum_key = if enum_name.contains('.') {
        enum_name.to_string()
    } else {
        qualify(module_alias, enum_name)
    };
    let alias = signatures.aliases.get(&enum_key)?;
    let TypeRef::Enum(tags) = &alias.body else {
        return None;
    };
    if !tags.contains_key(tag) {
        return None;
    }
    Some((enum_key, tag.to_string()))
}

/// Lower an enum-constructor call. `typed_enum_constructor_head` already
/// confirmed the enum and tag both exist, so the lookups below cannot
/// meaningfully fail; they stay defensive rather than `.expect()` so a
/// future change to that resolver fails as a diagnostic, not a panic.
/// Payload arity is checked here; payload *type* compatibility is deferred
/// to `validate_expr`, once locals/constants are available for inference.
#[allow(clippy::too_many_arguments)]
fn lower_enum_constructor_call(
    module_alias: &str,
    enum_key: &str,
    tag: &str,
    arguments: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Expr> {
    let alias = signatures
        .aliases
        .get(enum_key)
        .with_context(|| format!("unknown typed enum `{enum_key}`"))?;
    let TypeRef::Enum(tags) = &alias.body else {
        bail!("typed constructor target `{enum_key}` is not an enum");
    };
    let payload_ty = tags
        .get(tag)
        .with_context(|| format!("unknown typed enum tag `{tag}` for enum `{enum_key}`"))?;
    if *payload_ty == TypeRef::Void {
        if !arguments.is_empty() {
            bail!("typed constructor `{enum_key}.{tag}` does not take a payload");
        }
        return Ok(Expr::EnumConstructor {
            enum_key: enum_key.to_string(),
            tag: tag.to_string(),
            payload: None,
        });
    }
    if arguments.len() != 1 {
        bail!(
            "typed constructor `{enum_key}.{tag}` requires exactly one payload argument, got {}",
            arguments.len()
        );
    }
    let payload = lower_expr(
        module_alias,
        &arguments[0],
        signatures,
        declared_aliases,
        generics,
    )?;
    Ok(Expr::EnumConstructor {
        enum_key: enum_key.to_string(),
        tag: tag.to_string(),
        payload: Some(Box::new(payload)),
    })
}

/// Lower an expression-position `if` branch. `Expr::If` holds exactly one
/// value expression per branch (unlike `Statement::If`, which holds full
/// statement lists), matching the legacy `$if` expression envelope's bare
/// `then`/`else` exprs. An empty body (`(do)`) is `unit`, matching `do`'s
/// general semantics; a body with more than one form has no single value to
/// reduce to and is rejected explicitly rather than guessed at.
fn lower_if_branch(
    module_alias: &str,
    body: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Expr> {
    match body {
        [] => Ok(Expr::Value(RuntimeValue::Void)),
        [single] => lower_expr(module_alias, single, signatures, declared_aliases, generics),
        _ => bail!(
            "typed expression-position `if` branch must reduce to a single value; found {} forms",
            body.len()
        ),
    }
}

/// Lower a primitive-operator call. Only the structural arity is checked
/// here; operand-type validity and the return type are computed later in
/// `validate_expr`, once locals and constants are available for inference.
#[allow(clippy::too_many_arguments)]
fn lower_primitive_call(
    module_alias: &str,
    callee: &str,
    op: PrimitiveOp,
    arity: usize,
    arguments: &[AstExpr],
    signatures: &TypedSignatureIndex,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Expr> {
    if arguments.len() != arity {
        bail!(
            "E-OP-001: `{callee}` requires exactly {arity} operand{}, got {}",
            if arity == 1 { "" } else { "s" },
            arguments.len()
        );
    }
    let args = arguments
        .iter()
        .map(|argument| {
            lower_expr(
                module_alias,
                argument,
                signatures,
                declared_aliases,
                generics,
            )
        })
        .collect::<Result<_>>()?;
    Ok(Expr::Primitive {
        op,
        args,
        // Placeholders: replaced with the inferred operand/return types by
        // `validate_expr`, which runs once locals/constants are available.
        operand_type: TypeRef::Void,
        return_type: TypeRef::Void,
    })
}

/// The bare surface name for a primitive op, for diagnostics. Inverse of
/// `typed_primitive_op`.
fn typed_primitive_name(op: PrimitiveOp) -> &'static str {
    use PrimitiveOp::*;
    match op {
        Convert => unreachable!("$convert uses a dedicated envelope, not `PrimitiveOp` dispatch"),
        Add => "add",
        Subtract => "subtract",
        Multiply => "multiply",
        Divide => "divide",
        Remainder => "remainder",
        Negate => "negate",
        Equal => "equal",
        NotEqual => "not-equal",
        LessThan => "less-than",
        LessOrEqual => "less-or-equal",
        GreaterThan => "greater-than",
        GreaterOrEqual => "greater-or-equal",
        And => "and",
        Or => "or",
        Not => "not",
        BitAnd => "bit-and",
        BitOr => "bit-or",
        BitXor => "bit-xor",
        BitNot => "bit-not",
        ShiftLeft => "shift-left",
        ShiftRight => "shift-right",
    }
}

/// Per-op validity on the common operand type, ported from the legacy
/// `parse_primitive_expr` in `lower.rs`.
fn typed_primitive_valid_for(op: PrimitiveOp, operand_type: &TypeRef) -> bool {
    use PrimitiveOp::*;
    match op {
        Convert => unreachable!("$convert uses a dedicated envelope, not `PrimitiveOp` dispatch"),
        Add | Subtract | Multiply | Divide | Remainder | Negate => {
            primitive_numeric(operand_type)
                && !(matches!(op, Negate)
                    && matches!(
                        operand_type,
                        TypeRef::UInt8 | TypeRef::UInt16 | TypeRef::UInt32 | TypeRef::UInt64
                    ))
        }
        Equal | NotEqual => {
            primitive_numeric(operand_type) || matches!(operand_type, TypeRef::Bool | TypeRef::Str)
        }
        LessThan | LessOrEqual | GreaterThan | GreaterOrEqual => {
            primitive_numeric(operand_type) || operand_type == &TypeRef::Str
        }
        And | Or | Not => operand_type == &TypeRef::Bool,
        BitAnd | BitOr | BitXor | BitNot | ShiftLeft | ShiftRight => {
            primitive_integer(operand_type)
        }
    }
}

/// Return type for a primitive op given its common operand type, ported from
/// the legacy `parse_primitive_expr` in `lower.rs`.
fn typed_primitive_return_type(op: PrimitiveOp, operand_type: &TypeRef) -> TypeRef {
    use PrimitiveOp::*;
    if matches!(
        op,
        Equal | NotEqual | LessThan | LessOrEqual | GreaterThan | GreaterOrEqual | And | Or | Not
    ) {
        TypeRef::Bool
    } else {
        operand_type.clone()
    }
}

fn lower_pattern(
    module_alias: &str,
    pattern: &AstPattern,
    declared_aliases: &BTreeSet<String>,
    generics: &BTreeSet<String>,
) -> Result<Pattern> {
    let lower = |value| lower_pattern(module_alias, value, declared_aliases, generics);
    Ok(match &pattern.value {
        PatternKind::Literal(value) => Pattern::Literal(lower_literal(value)),
        PatternKind::Wildcard => Pattern::Wildcard,
        PatternKind::Bind(name) => Pattern::Bind(name.value.clone()),
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            let (enum_name, tag) = constructor.value.rsplit_once('.').with_context(|| {
                format!(
                    "typed constructor `{}` must include its enum name",
                    constructor.value
                )
            })?;
            if arguments.len() > 1 {
                bail!("typed enum patterns accept at most one payload");
            }
            Pattern::Enum {
                enum_key: if enum_name.contains('.') {
                    enum_name.to_string()
                } else {
                    qualify(module_alias, enum_name)
                },
                tag: tag.to_string(),
                payload: arguments.first().map(lower).transpose()?.map(Box::new),
            }
        }
        PatternKind::Record(fields) => Pattern::Record(
            fields
                .iter()
                .map(|field| Ok((field.name.value.clone(), lower(&field.pattern)?)))
                .collect::<Result<_>>()?,
        ),
        PatternKind::Tuple(items) => {
            Pattern::Tuple(items.iter().map(lower).collect::<Result<_>>()?)
        }
        PatternKind::Array(items) => {
            Pattern::Array(items.iter().map(lower).collect::<Result<_>>()?)
        }
        PatternKind::Map(items) => Pattern::Map(
            items
                .iter()
                .map(|(key, value)| Ok((lower(key)?, lower(value)?)))
                .collect::<Result<_>>()?,
        ),
        PatternKind::Newtype { ty, pattern } => Pattern::Newtype {
            type_ref: lower_type(ty, generics, module_alias, declared_aliases)?,
            inner: Box::new(lower(pattern)?),
        },
        PatternKind::Interface { .. } => {
            bail!("typed interface-pattern lowering is staged until its subpattern is preserved")
        }
    })
}

fn lower_literal(value: &Literal) -> RuntimeValue {
    match value {
        Literal::String(value) => RuntimeValue::Str(value.clone()),
        Literal::Bool(value) => RuntimeValue::Bool(*value),
        Literal::Int(value) => RuntimeValue::Int(*value),
        Literal::Float(value) => RuntimeValue::Float(*value),
        Literal::Unit => RuntimeValue::Void,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DocumentId, Module};
    use crate::lower::{Call, LoweredProgram};
    use crate::runtime::RunConfig;
    use crate::{ast, syntax};
    use std::collections::{BTreeMap, HashMap};

    fn module(source: &str) -> Module {
        let document = syntax::parse(source).unwrap();
        ast::lower_document_with_id(&document, DocumentId::from_raw(20)).unwrap()
    }

    #[test]
    fn lowers_staged_sexpression_function_constant_and_test_bodies() {
        let source = module(
            r#"(fn copy ((value int64)) int64
  (do (let-as copy int64 value) (return copy)))
(const answer int64 42)
(test smoke core (do (copy answer)))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let FunctionBody::User { statements } = &bodies.functions["copy"] else {
            panic!("expected user body");
        };
        assert_eq!(statements.len(), 2);
        assert!(matches!(statements[0], Statement::Let { .. }));
        assert!(matches!(
            bodies.staged_constant("answer").unwrap(),
            Expr::Value(RuntimeValue::Int(42))
        ));
        assert!(matches!(
            bodies.staged_test("smoke").unwrap()[0],
            Statement::Call(_)
        ));
        assert_eq!(
            bodies.function_origin("copy").unwrap().document,
            DocumentId::from_raw(20)
        );
    }

    #[test]
    fn identity_subset_executes_in_interpreter_and_wasm() {
        let source = module("(fn identity ((value int64)) int64 (do (return value)))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_identity_functions(&signatures, &bodies).unwrap();
        let program = LoweredProgram {
            statements: vec![Statement::Call(Call {
                callee_key: "identity".into(),
                type_args: Vec::new(),
                args: vec![Expr::Value(RuntimeValue::Int(7))],
            })],
            main_arg_bindings: Vec::new(),
            constants: HashMap::new(),
            functions,
            impls: HashMap::new(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        };
        crate::execute::run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn pure_calls_control_collections_and_mutation_execute_in_both_backends() {
        let source = module(
            r#"(fn choose ((flag bool) (left int64) (right int64)) int64
  (do (if flag (do (return left)) (do (return right)))))
(fn collect ((value int64)) (array int64)
  (do (let values (array value value)) (return values)))
(fn mutate ((value int64)) int64
  (do (let cell (mut value)) (set cell 9) (return cell)))
(fn forward ((value int64)) int64
  (do (return (choose true value answer))))
(fn observe ((value int64)) void
  (do (task (captures value) (do value))))
(fn loop-over ((value int64)) int64
  (do
    (while false (do (break)))
    (for item (array value) (do item))
    (return value)))
(const answer int64 42)"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let statements = [
            "choose",
            "collect",
            "mutate",
            "forward",
            "observe",
            "loop-over",
        ]
        .into_iter()
        .map(|callee| {
            let args = match callee {
                "choose" => vec![
                    Expr::Value(RuntimeValue::Bool(true)),
                    Expr::Value(RuntimeValue::Int(7)),
                    Expr::Value(RuntimeValue::Int(8)),
                ],
                _ => vec![Expr::Value(RuntimeValue::Int(7))],
            };
            Statement::Call(Call {
                callee_key: callee.into(),
                type_args: Vec::new(),
                args,
            })
        })
        .collect();
        let program = LoweredProgram {
            statements,
            main_arg_bindings: Vec::new(),
            constants: HashMap::from([("answer".into(), RuntimeValue::Int(42))]),
            functions,
            impls: HashMap::new(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        };
        crate::execute::run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn executable_subset_rejects_incomplete_or_unchecked_bodies() {
        let signatures = TypedSignatureIndex {
            imports: BTreeMap::new(),
            visibility: BTreeMap::new(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            constants: HashMap::new(),
            tests: HashMap::new(),
            impls: HashMap::new(),
        };
        let mut bodies = TypedBodyIndex::default();
        bodies
            .functions
            .insert("ghost".into(), FunctionBody::User { statements: vec![] });
        assert!(materialize_typed_identity_functions(&signatures, &bodies)
            .unwrap_err()
            .to_string()
            .contains("set mismatch"));

        for source in [
            "(fn bad ((value int64)) int64 (do (return missing)))",
            "(fn bad ((value int64)) bool (do (return value)))",
            "(fn bad ((value t)) t (do (return value)) where: ((t)))",
            "(fn bad ((value int64)) int64 (do (if value (do (return value)) (do (return value)))))",
            "(fn bad ((value int64)) int64 (do (break) (return value)))",
            "(fn bad ((value int64)) int64 (do (set value 2) (return value)))",
            "(fn bad ((value int64)) int64 (do (let-as copy bool value) (return copy)))",
            "(fn bad ((value int64)) void (do (let cell (mut value)) (task (captures cell) (do cell))))",
            "(fn target ((value int64)) int64 (do (return value))) (fn bad () int64 (do (return (target true))))",
        ] {
            let module = module(source);
            let inputs = [TypedModuleInput {
                alias: "",
                module: &module,
            }];
            let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
            let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
            assert!(
                materialize_typed_identity_functions(&signatures, &bodies).is_err(),
                "unexpectedly materialized {source}"
            );
        }
    }

    #[test]
    fn interface_patterns_are_not_lowered_lossily() {
        let source = module(
            "(fn bad ((value int64)) int64
               (do (match value (case (interface int64 (bind inner)) (do (return inner))))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = lower_typed_bodies(inputs, &signatures).unwrap_err();
        assert!(format!("{error:#}").contains("subpattern"));
    }

    #[test]
    fn imported_calls_and_constants_require_scope_and_public_visibility() {
        let library = module(
            r#"(fn echo ((value int64)) int64 (do (return value)))
(const answer int64 42)
(private (fn hidden ((value int64)) int64 (do (return value))))
(private (const secret int64 7))"#,
        );
        let entry = module(
            r#"(import lib "./lib.vibra")
(fn use-public ((value int64)) int64
  (do (let copy (lib.echo value)) (return lib.answer)))"#,
        );
        let inputs = [
            TypedModuleInput {
                alias: "lib",
                module: &library,
            },
            TypedModuleInput {
                alias: "",
                module: &entry,
            },
        ];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        materialize_typed_functions(&signatures, &bodies).unwrap();

        for reference in ["(lib.hidden value)", "lib.secret"] {
            let entry = module(&format!(
                "(import lib \"./lib.vibra\")\n(fn bad ((value int64)) int64 (do (return {reference})))"
            ));
            let inputs = [
                TypedModuleInput {
                    alias: "lib",
                    module: &library,
                },
                TypedModuleInput {
                    alias: "",
                    module: &entry,
                },
            ];
            let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
            let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
            assert!(materialize_typed_functions(&signatures, &bodies).is_err());
        }
    }

    #[test]
    fn validation_diagnostics_retain_macro_expansion_origins() {
        let expanded = crate::ast::expand_typed_macros(module(
            r#"(macro unrelated () expr-syntax
  (do (quote expr-syntax 1)))
(macro wrong () expr-syntax
  (do (quote expr-syntax true)))
(fn bad () int64 (do (unrelated) (return (wrong))))"#,
        ))
        .unwrap();
        let TopLevel::Function(function) = &expanded.forms[0] else {
            panic!("expected expanded function");
        };
        let unrelated_origin = &function.body[0].origin;
        let ExprKind::Return(Some(wrong)) = &function.body[1].value else {
            panic!("expected return");
        };
        let expected_origin = format_origin(&wrong.origin);
        let unrelated_origin = format_origin(unrelated_origin);
        let inputs = [TypedModuleInput {
            alias: "",
            module: &expanded,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = materialize_typed_functions(&signatures, &bodies).unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains(&expected_origin), "{error}");
        assert!(!error.contains(&unrelated_origin), "{error}");
        assert!(!error.contains("expression origins"), "{error}");
    }

    #[test]
    fn for_shadowing_selects_binding_statement_after_macro_source_validation() {
        let expanded = crate::ast::expand_typed_macros(module(
            r#"(macro source () expr-syntax
  (do (quote expr-syntax (array 1))))
(fn bad ((item int64)) void
  (do (for item (source) (do unit))))"#,
        ))
        .unwrap();
        let TopLevel::Function(function) = &expanded.forms[0] else {
            panic!("expected expanded function");
        };
        let statement_origin = format_origin(&function.body[0].origin);
        let ExprKind::For { source, .. } = &function.body[0].value else {
            panic!("expected for statement");
        };
        let source_origin = format_origin(&source.origin);
        let inputs = [TypedModuleInput {
            alias: "",
            module: &expanded,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains(&statement_origin), "{error}");
        assert!(!error.contains(&source_origin), "{error}");
    }

    #[test]
    fn range_validation_selects_the_exact_failing_bound_origin() {
        let expanded = crate::ast::expand_typed_macros(module(
            r#"(macro wrong-bound () expr-syntax
  (do (quote expr-syntax true)))
(fn bad () void (do (range 0 (wrong-bound) 1)))"#,
        ))
        .unwrap();
        let TopLevel::Function(function) = &expanded.forms[0] else {
            panic!("expected expanded function");
        };
        let range_origin = format_origin(&function.body[0].origin);
        let ExprKind::Range(_, end, _) = &function.body[0].value else {
            panic!("expected range expression");
        };
        let bound_origin = format_origin(&end.origin);
        let inputs = [TypedModuleInput {
            alias: "",
            module: &expanded,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains(&bound_origin), "{error}");
        assert!(!error.contains(&range_origin), "{error}");
    }

    fn primitive_return(functions: &HashMap<String, FunctionSig>, key: &str) -> Expr {
        let FunctionBody::User { statements } = &functions[key].body else {
            panic!("expected user body for `{key}`");
        };
        let Statement::Return(expr) = &statements[0] else {
            panic!("expected a return statement in `{key}`");
        };
        assert!(
            matches!(expr, Expr::Primitive { .. }),
            "expected a primitive expression in `{key}`, got {expr:?}"
        );
        expr.clone()
    }

    #[test]
    fn primitive_arithmetic_and_bitwise_ops_lower_with_operand_and_return_type() {
        let source = module(
            r#"(fn addition ((a int64) (b int64)) int64 (do (return (add a b))))
(fn subtraction ((a int64) (b int64)) int64 (do (return (subtract a b))))
(fn multiplication ((a int64) (b int64)) int64 (do (return (multiply a b))))
(fn division ((a int64) (b int64)) int64 (do (return (divide a b))))
(fn remaindering ((a int64) (b int64)) int64 (do (return (remainder a b))))
(fn negation ((a int64)) int64 (do (return (negate a))))
(fn bitwise-and ((a int64) (b int64)) int64 (do (return (bit-and a b))))
(fn bitwise-or ((a int64) (b int64)) int64 (do (return (bit-or a b))))
(fn bitwise-xor ((a int64) (b int64)) int64 (do (return (bit-xor a b))))
(fn bitwise-not ((a int64)) int64 (do (return (bit-not a))))
(fn shift-left-by ((a int64) (b int64)) int64 (do (return (shift-left a b))))
(fn shift-right-by ((a int64) (b int64)) int64 (do (return (shift-right a b))))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        for key in [
            "addition",
            "subtraction",
            "multiplication",
            "division",
            "remaindering",
            "negation",
            "bitwise-and",
            "bitwise-or",
            "bitwise-xor",
            "bitwise-not",
            "shift-left-by",
            "shift-right-by",
        ] {
            let Expr::Primitive {
                operand_type,
                return_type,
                ..
            } = primitive_return(&functions, key)
            else {
                unreachable!()
            };
            assert_eq!(operand_type, TypeRef::Int64, "{key}");
            assert_eq!(
                return_type,
                TypeRef::Int64,
                "{key} (arithmetic returns operand type)"
            );
        }
    }

    #[test]
    fn primitive_comparison_and_logical_ops_return_bool() {
        let source = module(
            r#"(fn less ((a int64) (b int64)) bool (do (return (less-than a b))))
(fn less-eq ((a int64) (b int64)) bool (do (return (less-or-equal a b))))
(fn greater ((a int64) (b int64)) bool (do (return (greater-than a b))))
(fn greater-eq ((a int64) (b int64)) bool (do (return (greater-or-equal a b))))
(fn eq ((a int64) (b int64)) bool (do (return (equal a b))))
(fn not-eq ((a int64) (b int64)) bool (do (return (not-equal a b))))
(fn str-cmp ((a str) (b str)) bool (do (return (less-than a b))))
(fn either ((a bool) (b bool)) bool (do (return (or a b))))
(fn both ((a bool) (b bool)) bool (do (return (and a b))))
(fn negation ((a bool)) bool (do (return (not a))))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        for key in [
            "less",
            "less-eq",
            "greater",
            "greater-eq",
            "eq",
            "not-eq",
            "str-cmp",
            "either",
            "both",
            "negation",
        ] {
            let Expr::Primitive { return_type, .. } = primitive_return(&functions, key) else {
                unreachable!()
            };
            assert_eq!(return_type, TypeRef::Bool, "{key}");
        }
    }

    #[test]
    fn primitive_call_as_a_let_value_and_as_a_bare_statement_lowers_and_executes() {
        let source = module(
            r#"(fn compute ((a int64) (b int64)) int64
  (do (let sum (add a b)) (add a b) (return sum)))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let program = LoweredProgram {
            statements: vec![Statement::Call(Call {
                callee_key: "compute".into(),
                type_args: Vec::new(),
                args: vec![
                    Expr::Value(RuntimeValue::Int(3)),
                    Expr::Value(RuntimeValue::Int(4)),
                ],
            })],
            main_arg_bindings: Vec::new(),
            constants: HashMap::new(),
            functions,
            impls: HashMap::new(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        };
        crate::execute::run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn primitive_arity_mismatch_is_rejected_at_lowering_time() {
        for source in [
            "(fn bad () int64 (do (return (add 1))))",
            "(fn bad () int64 (do (return (negate 1 2))))",
            "(fn bad () bool (do (return (not true false))))",
        ] {
            let source = module(source);
            let inputs = [TypedModuleInput {
                alias: "",
                module: &source,
            }];
            let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
            let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
            assert!(error.contains("E-OP-001"), "{error}");
            assert!(error.contains("operand"), "{error}");
        }
    }

    #[test]
    fn primitive_mixed_operand_types_require_an_explicit_cast() {
        let source = module("(fn bad ((a int64) (b bool)) int64 (do (return (add a b))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-OP-001"), "{error}");
        assert!(error.contains("cast"), "{error}");
        assert!(!error.contains("$cast"), "{error}");
    }

    #[test]
    fn negate_on_unsigned_operand_is_rejected() {
        let source = module("(fn bad ((a uint32)) uint32 (do (return (negate a))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-OP-001"), "{error}");
        assert!(error.contains("not defined for"), "{error}");
        assert!(error.contains("UInt32"), "{error}");
    }

    #[test]
    fn and_on_non_bool_operand_is_rejected() {
        let source = module("(fn bad ((a int64) (b int64)) bool (do (return (and a b))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-OP-001"), "{error}");
        assert!(error.contains("not defined for"), "{error}");
        assert!(error.contains("Int64"), "{error}");
    }

    #[test]
    fn bitwise_on_float_operand_is_rejected() {
        let source =
            module("(fn bad ((a float64) (b float64)) float64 (do (return (bit-and a b))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-OP-001"), "{error}");
        assert!(error.contains("not defined for"), "{error}");
        assert!(error.contains("Float64"), "{error}");
    }

    #[test]
    fn qualified_callee_is_never_treated_as_a_primitive() {
        // `lib` never declares `add`, so a qualified `lib.add` call must not
        // silently resolve to the `add` primitive: it must fail as an
        // unresolved callee.
        let library = module("(fn echo ((value int64)) int64 (do (return value)))");
        let entry = module(
            r#"(import lib "./lib.vibra")
(fn bad ((value int64)) int64 (do (return (lib.add value 1))))"#,
        );
        let inputs = [
            TypedModuleInput {
                alias: "lib",
                module: &library,
            },
            TypedModuleInput {
                alias: "",
                module: &entry,
            },
        ];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("lib.add"), "{error}");
        assert!(error.contains("unknown"), "{error}");
    }

    #[test]
    fn unresolved_callee_in_expression_position_is_a_diagnostic_not_a_panic() {
        // Exercises the branch that previously read
        // `signatures.functions.get(&call.callee_key).expect("resolved typed
        // call")`: this must surface as an `Err`, never as a panic.
        let source = module("(fn bad () int64 (do (return (totally-unknown-fn))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("totally-unknown-fn"), "{error}");
    }

    #[test]
    fn declaring_a_function_named_after_a_primitive_is_permitted() {
        // The standard library names combinators `option.and`, `option.or`,
        // `result.and`, and `result.or`. Those declarations are reachable
        // through their qualified names, so rejecting them would be wrong.
        let source = module("(fn and ((a int64) (b int64)) int64 (do (return a)))");
        let inputs = [TypedModuleInput {
            alias: "lib",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs)
            .expect("a declaration named after a primitive is reachable when qualified");
        assert!(signatures.functions.contains_key("lib.and"));
    }

    #[test]
    fn unqualified_primitive_head_wins_over_a_local_declaration_of_that_name() {
        // Primitive availability is uniform across modules: it must not depend
        // on what the enclosing module happens to declare. Inside a module that
        // declares `and`, an unqualified `(and ...)` is still the primitive.
        let source = module(
            "(fn and ((a bool) (b bool)) bool (do (return a)))\n\
             (fn use-it ((x bool) (y bool)) bool (do (return (and x y))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let Expr::Primitive { op, .. } = primitive_return(&functions, "use-it") else {
            unreachable!()
        };
        assert_eq!(op, PrimitiveOp::And);
    }

    // ---- Expr::EnumConstructor ----

    #[test]
    fn enum_constructor_lowers_and_validates_nullary_and_payload_tags() {
        let source = module(
            r#"(def color (enum (red void) (custom int64)))
(fn pick-red () int64
  (do (let chosen (color.red)) (return 1)))
(fn pick-custom ((value int64)) int64
  (do (let chosen (color.custom value)) (return value)))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();

        let FunctionBody::User { statements } = &functions["pick-red"].body else {
            panic!("expected user body");
        };
        let Statement::Let {
            value:
                LetValue::Expr(Expr::EnumConstructor {
                    enum_key,
                    tag,
                    payload,
                }),
            ..
        } = &statements[0]
        else {
            panic!(
                "expected enum constructor let value, got {:?}",
                statements[0]
            );
        };
        assert_eq!(enum_key, "color");
        assert_eq!(tag, "red");
        assert!(payload.is_none());

        let FunctionBody::User { statements } = &functions["pick-custom"].body else {
            panic!("expected user body");
        };
        let Statement::Let {
            value:
                LetValue::Expr(Expr::EnumConstructor {
                    enum_key,
                    tag,
                    payload,
                }),
            ..
        } = &statements[0]
        else {
            panic!(
                "expected enum constructor let value, got {:?}",
                statements[0]
            );
        };
        assert_eq!(enum_key, "color");
        assert_eq!(tag, "custom");
        assert!(payload.is_some());
    }

    #[test]
    fn enum_constructor_unknown_alias_falls_back_to_an_ordinary_unknown_callee_error() {
        // `nonexistent-type` never registers a type alias at all, so this
        // must not be silently misdiagnosed as an enum-constructor problem;
        // it falls back to ordinary call resolution and fails there.
        let source =
            module("(fn bad () int64 (do (let chosen (nonexistent-type.tag)) (return 1)))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("unknown typed callee"), "{error}");
        assert!(error.contains("nonexistent-type.tag"), "{error}");
    }

    #[test]
    fn enum_constructor_unknown_tag_falls_through_to_ordinary_call_and_still_errors() {
        // `color` is a real enum, but `mystery` is not one of its tags and
        // `bad` never declares a `mystery` function either. A prefix match
        // on the enum alone must not foreclose ordinary call resolution: it
        // falls through and fails there, as a genuinely unresolvable callee,
        // rather than misreporting an "unknown enum tag".
        let source = module(
            r#"(def color (enum (red void)))
(fn bad () int64 (do (let chosen (color.mystery)) (return 1)))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("unknown typed callee"), "{error}");
        assert!(error.contains("color.mystery"), "{error}");
    }

    #[test]
    fn enum_constructor_prefix_match_falls_through_to_a_real_qualified_function() {
        // Mirrors the exact stdlib shape that motivated this fallthrough:
        // `option.vibra` is compiled under its own import alias (`option`,
        // matching how importers refer to it), declares an enum named
        // `option` (tags `some`/`none`), and separately declares an
        // ordinary function named `empty`. Its own code then calls that
        // function via the fully self-qualified `(option.empty ...)` --
        // exactly the shape the corpus migrator emits for the legacy
        // `$option.empty: ...` call. This must resolve to the function, not
        // hard-error as a bogus `empty` tag on the `option` enum.
        let source = module(
            "(def option (enum (some int64) (none void)))
(fn empty ((ignored bool)) int64 (do (return 0)))
(fn caller () int64 (do (return (option.empty true))))",
        );
        let inputs = [TypedModuleInput {
            alias: "option",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let FunctionBody::User { statements } = &functions["option.caller"].body else {
            panic!("expected user body");
        };
        assert!(
            matches!(statements[0], Statement::Return(Expr::Call { .. })),
            "expected an ordinary call to `option.empty`, got {:?}",
            statements[0]
        );
    }

    #[test]
    fn enum_constructor_payload_arity_mismatches_are_rejected_at_lowering_time() {
        for source in [
            // `red` takes no payload.
            r#"(def color (enum (red void) (custom int64)))
(fn bad () int64 (do (let chosen (color.red 1)) (return 1)))"#,
            // `custom` requires exactly one payload.
            r#"(def color (enum (red void) (custom int64)))
(fn bad () int64 (do (let chosen (color.custom)) (return 1)))"#,
        ] {
            let source = module(source);
            let inputs = [TypedModuleInput {
                alias: "",
                module: &source,
            }];
            let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
            let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
            assert!(error.contains("payload"), "{error}");
        }
    }

    #[test]
    fn enum_constructor_payload_type_mismatch_is_rejected_at_validation_time() {
        let source = module(
            r#"(def color (enum (custom int64)))
(fn bad () int64 (do (let chosen (color.custom true)) (return 1)))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("payload type mismatch"), "{error}");
    }

    // ---- Expr::If ----

    #[test]
    fn expression_if_lowers_and_infers_the_common_branch_type() {
        let source = module(
            "(fn choose ((flag bool) (left int64) (right int64)) int64
               (do (return (if flag (do left) (do right)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let FunctionBody::User { statements } = &functions["choose"].body else {
            panic!("expected user body");
        };
        let Statement::Return(Expr::If { .. }) = &statements[0] else {
            panic!("expected expression-if in return, got {:?}", statements[0]);
        };
    }

    #[test]
    fn expression_if_with_empty_branches_is_unit() {
        let source = module("(fn noop ((flag bool)) void (do (return (if flag (do) (do)))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        materialize_typed_functions(&signatures, &bodies).unwrap();
    }

    #[test]
    fn expression_if_condition_must_be_bool() {
        let source = module(
            "(fn bad ((flag int64) (left int64) (right int64)) int64
               (do (return (if flag (do left) (do right)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("must be bool"), "{error}");
    }

    #[test]
    fn expression_if_branches_must_have_compatible_types() {
        let source = module(
            "(fn bad ((flag bool) (left int64) (right bool)) int64
               (do (return (if flag (do left) (do right)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("incompatible types"), "{error}");
    }

    #[test]
    fn expression_if_branch_with_more_than_one_form_is_rejected_at_lowering_time() {
        let source = module(
            "(fn bad ((flag bool) (left int64) (right int64)) int64
               (do (return (if flag (do left left) (do right)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("single value"), "{error}");
    }

    // ---- Expr::PolicyNarrow ----
    //
    // A policy-typed value can only ever exist as a function parameter, and
    // `ensure_safe_type` does not yet admit `TypeRef::Policy` at the function
    // signature boundary (that remains staged, same as capabilities,
    // interfaces, and enums as return/arg types). These tests therefore call
    // the private lowering/validation functions directly with a hand-built
    // `locals` map, exercising the same code the full pipeline would run
    // once a policy-typed value can legally reach a typed function body.

    #[test]
    fn policy_narrow_lowers_as_its_own_head_and_accepts_a_covering_narrow() {
        let source = module(
            "(fn narrow () bool (do (return (policy.narrow subject (capability fs-read)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let TopLevel::Function(function) = &source.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Return(Some(narrow_expr)) = &function.body[0].value else {
            panic!("expected return");
        };
        let lowered = lower_expr(
            "",
            narrow_expr,
            &signatures,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .unwrap();
        let Expr::PolicyNarrow { target, .. } = &lowered else {
            panic!("expected policy-narrow, got {lowered:?}");
        };
        assert!(matches!(target, TypeRef::Capability(_)), "{target:?}");

        let mut locals = HashMap::new();
        locals.insert(
            "subject".to_string(),
            TypeRef::Policy(crate::lower::PolicyType {
                domains: BTreeMap::from([(crate::lower::CapabilityDomain::FsRead, Vec::new())]),
            }),
        );
        let constants = HashMap::new();
        let mut node_origins = Vec::new();
        collect_expr_origin(narrow_expr, &mut node_origins);
        let mut origins = OriginCursor::new(&node_origins);
        let validated = validate_expr(
            &lowered,
            &locals,
            &constants,
            &signatures,
            "narrow",
            &mut origins,
        )
        .unwrap();
        origins.finish().unwrap();
        assert!(matches!(validated, Expr::PolicyNarrow { .. }));
    }

    #[test]
    fn policy_narrow_rejects_widening_authority() {
        let source = module(
            "(fn narrow () bool (do (return (policy.narrow subject (capability net-connect)))))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let TopLevel::Function(function) = &source.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Return(Some(narrow_expr)) = &function.body[0].value else {
            panic!("expected return");
        };
        let lowered = lower_expr(
            "",
            narrow_expr,
            &signatures,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .unwrap();

        let mut locals = HashMap::new();
        // `subject` only ever carried `fs-read` authority; narrowing into
        // `net-connect` must not be allowed.
        locals.insert(
            "subject".to_string(),
            TypeRef::Policy(crate::lower::PolicyType {
                domains: BTreeMap::from([(crate::lower::CapabilityDomain::FsRead, Vec::new())]),
            }),
        );
        let constants = HashMap::new();
        let mut node_origins = Vec::new();
        collect_expr_origin(narrow_expr, &mut node_origins);
        let mut origins = OriginCursor::new(&node_origins);
        let error = format!(
            "{:#}",
            validate_expr(
                &lowered,
                &locals,
                &constants,
                &signatures,
                "narrow",
                &mut origins,
            )
            .unwrap_err()
        );
        assert!(error.contains("cannot widen authority"), "{error}");
    }

    #[test]
    fn policy_narrow_on_a_non_policy_target_is_a_distinct_diagnostic() {
        // `policy.narrow` is a distinct head from `cast` precisely so a
        // narrow into a non-policy type stays honest: it must not silently
        // succeed as an ordinary cast, nor be misreported as a widening
        // failure -- it is a shape error on the target type itself.
        let source = module("(fn bad () int64 (do (return (policy.narrow subject int64))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(
            error.contains("must be a capability or policy type"),
            "{error}"
        );
    }

    #[test]
    fn cast_to_a_capability_type_is_not_silently_reinterpreted_as_policy_narrow() {
        // Regression guard: `cast` must stay a plain cast even when its
        // target happens to be a capability/policy type. Only the explicit
        // `policy.narrow` head performs subsumption checking. `cast`
        // remains staged, so this must fail as a (still-staged) cast, not
        // succeed as an implicit narrow.
        let source = module("(fn bad () bool (do (return (cast subject (capability fs-read)))))");
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let FunctionBody::User { statements } = &bodies.functions["bad"] else {
            panic!("expected user body");
        };
        let Statement::Return(Expr::Cast { .. }) = &statements[0] else {
            panic!("expected a plain cast, got {:?}", statements[0]);
        };
    }

    // ---- Statement::Spawn / Statement::Join ----

    #[test]
    fn spawn_and_join_lower_validate_and_execute_in_both_backends() {
        let source = module(
            "(fn spawn-and-join ((value int64)) int64
               (do
                 (spawn worker (captures value) (add value 1))
                 (join worker outcome)
                 (return outcome)))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        let FunctionBody::User { statements } = &functions["spawn-and-join"].body else {
            panic!("expected user body");
        };
        let Statement::Spawn {
            handle,
            captures,
            result_type,
            ..
        } = &statements[0]
        else {
            panic!("expected spawn, got {:?}", statements[0]);
        };
        assert_eq!(handle, "worker");
        assert_eq!(captures, &vec!["args.value".to_string()]);
        assert_eq!(result_type, &TypeRef::Int64);
        let Statement::Join { handle, var } = &statements[1] else {
            panic!("expected join, got {:?}", statements[1]);
        };
        assert_eq!(handle, "worker");
        assert_eq!(var, "outcome");

        let program = LoweredProgram {
            statements: vec![Statement::Call(Call {
                callee_key: "spawn-and-join".into(),
                type_args: Vec::new(),
                args: vec![Expr::Value(RuntimeValue::Int(41))],
            })],
            main_arg_bindings: Vec::new(),
            constants: HashMap::new(),
            functions,
            impls: HashMap::new(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        };
        crate::execute::run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn spawn_rejects_mutable_captures_and_non_void_result() {
        for source in [
            // `cell` is mutable; captures must be immutable snapshots.
            "(fn bad ((value int64)) int64
               (do (let cell (mut value)) (spawn worker (captures cell) value) (join worker outcome) (return outcome)))",
            // A spawned computation must produce a non-void result.
            "(fn bad () int64
               (do (spawn worker (captures) unit) (join worker outcome) (return 1)))",
        ] {
            let source = module(source);
            let inputs = [TypedModuleInput {
                alias: "",
                module: &source,
            }];
            let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
            let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
            assert!(
                materialize_typed_functions(&signatures, &bodies).is_err(),
                "unexpectedly materialized {source:?}"
            );
        }
    }

    #[test]
    fn join_requires_a_live_unjoined_handle() {
        let source = module(
            "(fn bad ((value int64)) int64
               (do (join worker outcome) (return value)))",
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("unknown or was already joined"), "{error}");
    }

    #[test]
    fn wasm_import_body_lowers_with_correct_signature_and_executes_in_both_backends() {
        let source = module(
            r#"(fn scalar-len ((value str)) uint64
  (do (wasm import: (import "vibra_v1" "str_scalar_len") args: ((arg value)))))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();

        // The staged IR is a `FunctionBody::Wasm`, not a statement sequence,
        // and it recorded no node origins (there is no `Statement`/`Expr`
        // walk for a wasm-only body -- see `lower_function`).
        let FunctionBody::Wasm { import, wasm_args } = &bodies.functions["scalar-len"] else {
            panic!("expected a wasm import body");
        };
        assert_eq!(import.module, "vibra_v1");
        assert_eq!(import.name, "str_scalar_len");
        assert_eq!(wasm_args.len(), 1);
        assert!(matches!(&wasm_args[0], WasmArgSpec::Arg(name) if name == "value"));
        assert!(bodies.node_origins["scalar-len"].is_empty());

        let functions = materialize_typed_functions(&signatures, &bodies).unwrap();
        assert!(matches!(
            &functions["scalar-len"].body,
            FunctionBody::Wasm { import, .. }
                if import.module == "vibra_v1" && import.name == "str_scalar_len"
        ));

        let program = LoweredProgram {
            statements: vec![Statement::Call(Call {
                callee_key: "scalar-len".into(),
                type_args: Vec::new(),
                args: vec![Expr::Value(RuntimeValue::Str("hi".into()))],
            })],
            main_arg_bindings: Vec::new(),
            constants: HashMap::new(),
            functions,
            impls: HashMap::new(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        };
        crate::execute::run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn wasm_import_argument_type_mismatch_is_rejected() {
        // `str_scalar_len` requires a `str` in position 0; `bool` must be
        // rejected the same way the legacy `$wasm` path rejects it
        // (`E-WASM-003`), not silently coerced or accepted.
        let source = module(
            r#"(fn bad ((value bool)) uint64
  (do (wasm import: (import "vibra_v1" "str_scalar_len") args: ((arg value)))))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-WASM-003"), "{error}");
    }

    #[test]
    fn wasm_import_targeting_an_unknown_host_module_is_rejected() {
        // A malformed import declaration -- one that names no registered
        // host module -- must fail strictly (`E-WASM-002`) rather than be
        // accepted on the strength of syntactic well-formedness alone; a
        // `$wasm` body crosses a trust boundary out to host functions.
        let source = module(
            r#"(fn bad () void
  (do (wasm import: (import "not-a-real-host-module" "whatever") args: ())))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = format!(
            "{:#}",
            materialize_typed_functions(&signatures, &bodies).unwrap_err()
        );
        assert!(error.contains("E-WASM-002"), "{error}");
    }

    #[test]
    fn wasm_import_nested_in_a_larger_body_is_rejected_explicitly() {
        // `$wasm` is a function *body kind*, matching the legacy path's
        // `is_wasm_only_body`/`extract_wasm_body`: it only lowers when it is
        // the function's entire body. Appearing alongside another statement
        // has no legacy equivalent and must fail explicitly, not be
        // silently accepted or panic.
        let source = module(
            r#"(fn bad ((flag bool)) void
  (do
    (wasm import: (import "vibra_test" "assert") args: ((arg flag)))
    (wasm import: (import "vibra_test" "assert") args: ((arg flag)))))"#,
        );
        let inputs = [TypedModuleInput {
            alias: "",
            module: &source,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let error = format!("{:#}", lower_typed_bodies(inputs, &signatures).unwrap_err());
        assert!(error.contains("entire body"), "{error}");
    }
}
