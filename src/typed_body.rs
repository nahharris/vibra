//! Executable lowering from typed S-expression AST nodes.
//!
//! This is separate from `typed_lower`: declarations become executable only
//! after every expression in their body has lowered successfully.

use crate::ast::{
    AnnotationKind, Expr as AstExpr, ExprKind, Function, Literal, Module, Pattern as AstPattern,
    PatternKind, SourceLocation, TopLevel,
};
use crate::lower::{
    Call, Expr, FunctionBody, FunctionSig, LetValue, MatchArm, Pattern, RuntimeValue, Statement,
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

/// Materialize the intentionally tiny, proven-safe executable subset.
///
/// Until the typed body checker covers the whole language, only a single
/// `return <parameter>` identity body is executable. All other successfully
/// parsed bodies remain staged and cannot become `FunctionSig`.
pub fn materialize_typed_identity_functions(
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
    bodies.functions.iter().map(|(key, body)| {
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
            let FunctionBody::User { statements } = body else {
                bail!(
                    "typed staged body `{key}` unexpectedly contains a wasm import {location}"
                );
            };
            let [Statement::Return(Expr::VarRef(reference))] = statements.as_slice() else {
                bail!(
                    "typed executable subset only supports a single parameter return in `{key}`; calls (including primitives, enum constructors, interface dispatch, imported calls), locals, and control forms remain non-executable {location}"
                );
            };
            let Some(index) = signature.arg_names.iter().position(|name| name == reference) else {
                bail!(
                    "typed identity return `{reference}` in `{key}` is not a declared parameter or safely materialized constant {location}"
                );
            };
            let argument_type = &signature.arg_types[index];
            if argument_type != &signature.return_type {
                bail!(
                    "typed identity `{key}` returns {:?} from parameter `{reference}`, but declares {:?} {location}",
                    argument_type,
                    signature.return_type,
                );
            }
            let checked = FunctionBody::User {
                statements: vec![Statement::Return(Expr::VarRef(format!(
                    "args.{reference}"
                )))],
            };
            Ok((key.clone(), materialize(signature, checked)))
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
            "(fn bad ((value int64)) int64 (do (let copy value) (return copy)))",
            "(fn bad ((value int64)) int64 (do (return (bad value))))",
            "(fn bad ((value int64)) bool (do (return value)))",
            "(fn bad ((value t)) t (do (return value)) where: ((t)))",
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
}
