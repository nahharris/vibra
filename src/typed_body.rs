//! Executable lowering from typed S-expression AST nodes.
//!
//! This is separate from `typed_lower`: declarations become executable only
//! after every expression in their body has lowered successfully.

use crate::ast::{
    AnnotationKind, Expr as AstExpr, ExprKind, Function, Literal, Module, Origin,
    Pattern as AstPattern, PatternKind, SourceLocation, TopLevel,
};
use crate::body_semantics::{validate_function_body, validate_task_handles};
use crate::lower::{
    infer_expr_type, Call, Expr, FunctionBody, FunctionSig, LetValue, MatchArm, Pattern,
    RuntimeValue, Statement, TypeRef,
};
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
    expression_origins: HashMap<String, Vec<Origin>>,
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
            let origin_chain = bodies
                .expression_origins
                .get(key)
                .map(|origins| format!("; expression origins {origins:?}"))
                .unwrap_or_default();
            if !signature.type_params.is_empty() {
                bail!(
                    "typed executable subset does not support generic function `{key}` {location}"
                );
            }
            for ty in signature.arg_types.iter().chain([&signature.return_type]) {
                ensure_safe_type(ty, key, &location)?;
            }
            let FunctionBody::User { statements } = body else {
                bail!("typed staged body `{key}` unexpectedly contains a wasm import {location}");
            };
            let mut locals: HashMap<_, _> = signature
                .arg_names
                .iter()
                .cloned()
                .zip(signature.arg_types.iter().cloned())
                .collect();
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
            )
            .with_context(|| {
                format!("validating typed function `{key}` {location}{origin_chain}")
            })?;
            validate_function_body(&checked, &signature.return_type)
                .with_context(|| format!("validating returns in typed function `{key}`"))?;
            validate_task_handles(&checked)
                .with_context(|| format!("validating task handles in typed function `{key}`"))?;
            let checked = FunctionBody::User {
                statements: checked,
            };
            Ok((key.clone(), materialize(signature, checked)))
        })
        .collect()
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
) -> Result<Vec<Statement>> {
    let mut checked = Vec::with_capacity(statements.len());
    for statement in statements {
        let statement = match statement {
            Statement::Call(call) => {
                Statement::Call(validate_call(call, locals, constants, signatures, context)?)
            }
            Statement::Let { var, value } => {
                if locals.contains_key(var) {
                    bail!("typed local `{var}` is already bound in `{context}`");
                }
                let (value, ty) = match value {
                    LetValue::Expr(expr) => {
                        let expr = validate_expr(expr, locals, constants, signatures, context)?;
                        let ty = infer(&expr, locals, constants, context)?;
                        (LetValue::Expr(expr), ty)
                    }
                    LetValue::Call(call) => {
                        let call = validate_call(call, locals, constants, signatures, context)?;
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
                let value = validate_expr(value, locals, constants, signatures, context)?;
                let actual = infer(&value, locals, constants, context)?;
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
                let expr = validate_expr(expr, locals, constants, signatures, context)?;
                let actual = infer(&expr, locals, constants, context)?;
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
                let cond = validate_expr(cond, locals, constants, signatures, context)?;
                if infer(&cond, locals, constants, context)? != TypeRef::Bool {
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
                )?;
                Statement::If {
                    cond,
                    then_body,
                    else_body,
                }
            }
            Statement::While { cond, body } => {
                let cond = validate_expr(cond, locals, constants, signatures, context)?;
                if infer(&cond, locals, constants, context)? != TypeRef::Bool {
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
                )?;
                Statement::While { cond, body }
            }
            Statement::For { var, source, body } => {
                let source = validate_expr(source, locals, constants, signatures, context)?;
                let item = match infer(&source, locals, constants, context)? {
                    TypeRef::Array(item) => *item,
                    TypeRef::Range => TypeRef::Int64,
                    other => bail!("typed for source must be array or range, got {other:?}"),
                };
                let mut nested = locals.clone();
                if nested.insert(var.clone(), item).is_some() {
                    bail!("typed for binding `{var}` shadows an existing local");
                }
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
            Statement::Eval(expr) => {
                Statement::Eval(validate_expr(expr, locals, constants, signatures, context)?)
            }
            Statement::Match { .. } => {
                bail!("typed match remains staged with enum and interface semantics")
            }
            Statement::Spawn { .. } | Statement::Join { .. } => {
                bail!("typed spawn/join remains staged until affine result typing is complete")
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
) -> Result<Call> {
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
            let argument = validate_expr(argument, locals, constants, signatures, context)?;
            let actual = infer(&argument, locals, constants, context)?;
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
) -> Result<Expr> {
    let recurse = |expr| validate_expr(expr, locals, constants, signatures, context);
    Ok(match expr {
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
            call: Box::new(validate_call(call, locals, constants, signatures, context)?),
            return_type: return_type.clone(),
        },
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(name, value)| Ok((name.clone(), recurse(value)?)))
                .collect::<Result<_>>()?,
        ),
        Expr::Tuple(items) => Expr::Tuple(items.iter().map(recurse).collect::<Result<_>>()?),
        Expr::Array(items) => Expr::Array(items.iter().map(recurse).collect::<Result<_>>()?),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| Ok((recurse(key)?, recurse(value)?)))
                .collect::<Result<_>>()?,
        ),
        Expr::Mutable(inner) => Expr::Mutable(Box::new(recurse(inner)?)),
        Expr::Reference { target, mutable } => Expr::Reference {
            target: Box::new(recurse(target)?),
            mutable: *mutable,
        },
        Expr::Range { start, end, step } => {
            let start = recurse(start)?;
            let end = recurse(end)?;
            let step = recurse(step)?;
            for value in [&start, &end, &step] {
                if infer(value, locals, constants, context)? != TypeRef::Int64 {
                    bail!("typed range bounds in `{context}` must be int64");
                }
            }
            Expr::Range {
                start: Box::new(start),
                end: Box::new(end),
                step: Box::new(step),
            }
        }
        Expr::Primitive { .. } => bail!("typed primitive operations remain staged"),
        Expr::EnumConstructor { .. } => bail!("typed enum constructors remain staged"),
        Expr::Cast { .. } | Expr::PolicyNarrow { .. } | Expr::If { .. } => {
            bail!("typed cast, policy, and expression-if forms remain staged")
        }
    })
}

fn infer(
    expr: &Expr,
    locals: &HashMap<String, TypeRef>,
    constants: &HashMap<String, RuntimeValue>,
    context: &str,
) -> Result<TypeRef> {
    let mut inference_locals = locals.clone();
    inference_locals.extend(
        locals
            .iter()
            .map(|(name, ty)| (format!("args.{name}"), ty.clone())),
    );
    infer_expr_type(
        expr,
        constants,
        &inference_locals,
        &HashMap::new(),
        &HashMap::new(),
    )
    .with_context(|| format!("cannot infer typed expression in `{context}`"))
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
    let mut expression_origins = Vec::new();
    collect_body_metadata(
        &function.body,
        module_alias,
        declared_aliases,
        &generic_names,
        &mut let_types,
        &mut lexical_bindings,
        &mut expression_origins,
    )?;
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
    bodies
        .expression_origins
        .insert(key.to_string(), expression_origins);
    Ok(())
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
    origins: &mut Vec<Origin>,
) -> Result<()> {
    for expression in expressions {
        origins.push(expression.origin.clone());
        match &expression.value {
            ExprKind::Do(body) | ExprKind::While { body, .. } | ExprKind::Task { body, .. } => {
                collect_body_metadata(
                    body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                    origins,
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
                    origins,
                )?;
                collect_body_metadata(
                    else_body,
                    module_alias,
                    declared_aliases,
                    generics,
                    let_types,
                    bindings,
                    origins,
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
                        origins,
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
            | ExprKind::Convert { value, .. } => collect_body_metadata(
                std::slice::from_ref(value.as_ref()),
                module_alias,
                declared_aliases,
                generics,
                let_types,
                bindings,
                origins,
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
                    origins,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
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
        ExprKind::Call { callee, arguments } => Statement::Call(lower_call(
            module_alias,
            &callee.value,
            arguments,
            signatures,
            declared_aliases,
            generics,
        )?),
        ExprKind::Let { name, ty, value } => {
            if let Some(ty) = ty {
                let _ = lower_type(ty, generics, module_alias, declared_aliases)?;
            }
            let value = match &value.value {
                ExprKind::Call { callee, arguments } => LetValue::Call(lower_call(
                    module_alias,
                    &callee.value,
                    arguments,
                    signatures,
                    declared_aliases,
                    generics,
                )?),
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
        ExprKind::Spawn { .. } => {
            bail!("typed spawn lowering is staged until full result-type checking is available")
        }
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
        ExprKind::Call { callee, arguments } => {
            let call = lower_call(
                module_alias,
                &callee.value,
                arguments,
                signatures,
                declared_aliases,
                generics,
            )?;
            let return_type = signatures
                .functions
                .get(&call.callee_key)
                .expect("resolved typed call")
                .return_type
                .clone();
            Expr::Call {
                call: Box::new(call),
                return_type,
            }
        }
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
        ExprKind::Convert { .. } => {
            bail!("typed `convert` lowering requires explicit fallback semantics")
        }
        ExprKind::Embed { .. } | ExprKind::Template { .. } | ExprKind::Wasm { .. } => {
            bail!("typed compile-time expression lowering is not active")
        }
        ExprKind::Do(_)
        | ExprKind::Let { .. }
        | ExprKind::Set { .. }
        | ExprKind::Return(_)
        | ExprKind::If { .. }
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
            r#"(macro wrong () expr-syntax
  (do (quote expr-syntax true)))
(fn bad () int64 (do (return (wrong))))"#,
        ))
        .unwrap();
        let inputs = [TypedModuleInput {
            alias: "",
            module: &expanded,
        }];
        let signatures = crate::typed_lower::lower_typed_signatures(inputs).unwrap();
        let bodies = lower_typed_bodies(inputs, &signatures).unwrap();
        let error = materialize_typed_functions(&signatures, &bodies).unwrap_err();
        assert!(format!("{error:#}").contains("DocumentExpansion"));
    }
}
