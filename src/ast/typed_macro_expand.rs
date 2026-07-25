//! Deterministic macro expansion over the native typed surface tree.
//!
//! This module deliberately stays dormant until the native frontend owns the
//! compiler pipeline. It never serializes typed syntax through YAML or generic
//! structural values.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use crate::syntax::{Atom, Node, Span};

use super::{
    lower_expression_node_with_id, lower_pattern_node_with_id, lower_top_level_node_with_id,
    lower_type_node_with_id, AstError, DocumentId, Expr, ExprKind, Function, Macro, MacroExpr,
    MacroExprKind, Module, Origin, Pattern, PatternKind, SourceLocation, Spanned, SyntaxCategory,
    TopLevel, TypeExpr, TypeExprKind, Visibility,
};

// Keep recursive expansion below the smallest supported native thread stack.
pub const TYPED_MACRO_MAX_EXPANSION_DEPTH: usize = 16;
pub const TYPED_MACRO_MAX_EVALUATION_STEPS: usize = 1_000_000;
pub const TYPED_MACRO_MAX_GENERATED_NODES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroExpansionError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub document: Option<DocumentId>,
    pub related: Vec<(String, SourceLocation)>,
}

impl MacroExpansionError {
    fn at(code: &'static str, message: impl Into<String>, origin: &Origin) -> Self {
        Self {
            code,
            message: message.into(),
            span: origin.primary_span(),
            document: origin.document_id(),
            related: Vec::new(),
        }
    }

    fn related(mut self, label: impl Into<String>, origin: &Origin) -> Self {
        if let Some(document) = origin.document_id() {
            self.related.push((
                label.into(),
                SourceLocation {
                    document,
                    span: origin.primary_span(),
                },
            ));
        }
        self
    }

    fn from_ast(error: AstError, document: DocumentId) -> Self {
        Self {
            code: error.code,
            message: error.message,
            span: error.span,
            document: Some(document),
            related: error
                .related
                .into_iter()
                .map(|(label, span)| (label, SourceLocation { document, span }))
                .collect(),
        }
    }
}

impl fmt::Display for MacroExpansionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}: {}",
            self.code, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for MacroExpansionError {}

#[derive(Debug, Clone)]
enum SyntaxFragment {
    Expression(Expr),
    Type(TypeExpr),
    Pattern(Pattern),
    Definition(TopLevel),
    Module(Vec<TopLevel>),
    Bool(bool),
}

impl SyntaxFragment {
    fn category(&self) -> Option<SyntaxCategory> {
        match self {
            Self::Expression(_) => Some(SyntaxCategory::Expression),
            Self::Type(_) => Some(SyntaxCategory::Type),
            Self::Pattern(_) => Some(SyntaxCategory::Pattern),
            Self::Definition(value) => {
                let _ = value;
                Some(SyntaxCategory::Definition)
            }
            Self::Module(values) => {
                let _ = values;
                Some(SyntaxCategory::Module)
            }
            Self::Bool(_) => None,
        }
    }
}

#[derive(Default)]
struct ExpansionState {
    evaluation_steps: usize,
    generated_nodes: usize,
    hygiene_id: usize,
}

#[derive(Debug, Clone)]
struct MacroEntry {
    definition: Macro,
    definition_symbols: BTreeSet<String>,
    caller_alias: Option<String>,
}

type MacroTable = BTreeMap<String, MacroEntry>;

/// Expand a complete typed program while preserving its physical AST modules.
///
/// The outer key is the canonical logical module path and each vector contains
/// that module's physical base/conditional parts. Imports map caller module
/// paths to their resolved direct aliases.
pub fn expand_typed_macro_program(
    modules: BTreeMap<PathBuf, Vec<Module>>,
    imports: &BTreeMap<PathBuf, BTreeMap<String, PathBuf>>,
) -> Result<BTreeMap<PathBuf, Vec<Module>>, MacroExpansionError> {
    let registry = collect_program_macros(&modules)?;
    let symbols = collect_program_symbols(&modules);
    let mut state = ExpansionState::default();
    let mut expanded = BTreeMap::new();
    for (module_path, parts) in modules {
        let table = macro_table_for_module(&module_path, imports, &registry, &symbols);
        let parts = parts
            .into_iter()
            .map(|part| expand_module_with_table(part, &table, &mut state))
            .collect::<Result<Vec<_>, _>>()?;
        expanded.insert(module_path, parts);
    }
    Ok(expanded)
}

/// Convenience wrapper for detached single-module callers.
pub fn expand_typed_macros(module: Module) -> Result<Module, MacroExpansionError> {
    let path = PathBuf::from("<detached>");
    let modules = BTreeMap::from([(path.clone(), vec![module])]);
    let mut expanded = expand_typed_macro_program(modules, &BTreeMap::new())?;
    Ok(expanded
        .remove(&path)
        .expect("detached module remains in expansion program")
        .pop()
        .expect("detached expansion has one physical module"))
}

fn collect_program_macros(
    modules: &BTreeMap<PathBuf, Vec<Module>>,
) -> Result<BTreeMap<(PathBuf, String), Macro>, MacroExpansionError> {
    let mut registry = BTreeMap::new();
    for (module_path, parts) in modules {
        for definition in parts
            .iter()
            .flat_map(|part| &part.forms)
            .filter_map(|form| {
                if let TopLevel::Macro(definition) = form {
                    Some(definition)
                } else {
                    None
                }
            })
        {
            if matches!(
                definition.result.value,
                SyntaxCategory::Definition | SyntaxCategory::Module
            ) || definition.parameters.iter().any(|parameter| {
                matches!(
                    parameter.category.value,
                    SyntaxCategory::Definition | SyntaxCategory::Module
                )
            }) {
                return Err(MacroExpansionError::at(
                    "E-MACRO-011",
                    "definition-syntax and module-syntax macros are not supported by the typed expander",
                    &definition.origin,
                ));
            }
            let key = (module_path.clone(), definition.name.value.clone());
            if registry.insert(key, definition.clone()).is_some() {
                return Err(MacroExpansionError::at(
                    "E-MOD-002",
                    format!(
                        "duplicate macro `{}` across physical module parts",
                        definition.name.value
                    ),
                    &definition.origin,
                ));
            }
        }
    }
    Ok(registry)
}

fn collect_program_symbols(
    modules: &BTreeMap<PathBuf, Vec<Module>>,
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    modules
        .iter()
        .map(|(path, parts)| {
            let names = parts
                .iter()
                .flat_map(|part| &part.forms)
                .filter_map(top_level_symbol)
                .map(str::to_string)
                .collect();
            (path.clone(), names)
        })
        .collect()
}

fn top_level_symbol(form: &TopLevel) -> Option<&str> {
    match form {
        TopLevel::Import(value) => Some(&value.alias.value),
        TopLevel::Definition(value) => Some(&value.name.value),
        TopLevel::Constant(value) => Some(&value.name.value),
        TopLevel::Function(value) => Some(&value.name.value),
        TopLevel::Macro(value) => Some(&value.name.value),
        TopLevel::Test(value) => Some(&value.name.value),
    }
}

fn macro_table_for_module(
    module_path: &Path,
    imports: &BTreeMap<PathBuf, BTreeMap<String, PathBuf>>,
    registry: &BTreeMap<(PathBuf, String), Macro>,
    symbols: &BTreeMap<PathBuf, BTreeSet<String>>,
) -> MacroTable {
    let mut table = MacroTable::new();
    for ((owner, name), definition) in registry {
        if owner == module_path {
            table.insert(
                name.clone(),
                MacroEntry {
                    definition: definition.clone(),
                    definition_symbols: symbols.get(owner).cloned().unwrap_or_default(),
                    caller_alias: None,
                },
            );
        }
    }
    if let Some(direct) = imports.get(module_path) {
        for (alias, target) in direct {
            for ((owner, name), definition) in registry {
                if owner == target {
                    table.insert(
                        format!("{alias}.{name}"),
                        MacroEntry {
                            definition: definition.clone(),
                            definition_symbols: symbols.get(owner).cloned().unwrap_or_default(),
                            caller_alias: Some(alias.clone()),
                        },
                    );
                }
            }
        }
    }
    table
}

fn expand_module_with_table(
    module: Module,
    macros: &MacroTable,
    state: &mut ExpansionState,
) -> Result<Module, MacroExpansionError> {
    let mut forms = Vec::new();
    for form in module.forms {
        if matches!(form, TopLevel::Macro(_)) {
            continue;
        }
        forms.push(expand_top_level(form, macros, state, 0)?);
    }
    Ok(Module { forms, ..module })
}

fn expand_top_level(
    form: TopLevel,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<TopLevel, MacroExpansionError> {
    Ok(match form {
        TopLevel::Import(import) => TopLevel::Import(import),
        TopLevel::Definition(mut definition) => {
            definition.body = expand_type(definition.body, macros, state, depth)?;
            TopLevel::Definition(definition)
        }
        TopLevel::Constant(mut constant) => {
            constant.ty = expand_type(constant.ty, macros, state, depth)?;
            constant.value = expand_expr(constant.value, macros, state, depth)?;
            TopLevel::Constant(constant)
        }
        TopLevel::Function(mut function) => {
            expand_function(&mut function, macros, state, depth)?;
            TopLevel::Function(function)
        }
        TopLevel::Test(mut test) => {
            test.body = expand_exprs(test.body, macros, state, depth)?;
            TopLevel::Test(test)
        }
        TopLevel::Macro(_) => unreachable!("macro definitions are removed before expansion"),
    })
}

fn expand_function(
    function: &mut Function,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<(), MacroExpansionError> {
    for parameter in &mut function.parameters {
        parameter.ty = expand_type(parameter.ty.clone(), macros, state, depth)?;
    }
    function.return_type = expand_type(function.return_type.clone(), macros, state, depth)?;
    function.body = expand_exprs(std::mem::take(&mut function.body), macros, state, depth)?;
    Ok(())
}

fn expand_expr(
    mut expression: Expr,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<Expr, MacroExpansionError> {
    if let ExprKind::Call { callee, arguments } = &expression.value {
        if let Some(entry) = macros.get(&callee.value) {
            let definition = &entry.definition;
            if entry.caller_alias.is_some() && definition.visibility == Visibility::Private {
                return Err(MacroExpansionError::at(
                    "E-MACRO-010",
                    format!("macro `{}` is private", callee.value),
                    &expression.origin,
                )
                .related("private macro defined here", &definition.origin));
            }
            if depth >= TYPED_MACRO_MAX_EXPANSION_DEPTH {
                return Err(MacroExpansionError::at(
                    "E-MACRO-006",
                    format!(
                        "macro expansion depth exceeded {} at `{}`",
                        TYPED_MACRO_MAX_EXPANSION_DEPTH, callee.value
                    ),
                    &expression.origin,
                )
                .related("macro defined here", &definition.origin));
            }
            let arguments = arguments
                .iter()
                .cloned()
                .map(SyntaxFragment::Expression)
                .collect();
            let fragment = invoke(
                definition,
                arguments,
                &expression.origin,
                state,
                entry.caller_alias.as_deref(),
                &entry.definition_symbols,
            )?;
            let SyntaxFragment::Expression(mut expanded) = fragment else {
                return Err(category_error(
                    definition,
                    SyntaxCategory::Expression,
                    &expression.origin,
                ));
            };
            annotate_generated_expr(&mut expanded, definition, &expression.origin, state)?;
            return expand_expr(expanded, macros, state, depth + 1);
        }
    }

    expression.value = match expression.value {
        ExprKind::Call { callee, arguments } => ExprKind::Call {
            callee,
            arguments: expand_exprs(arguments, macros, state, depth)?,
        },
        ExprKind::Do(values) => ExprKind::Do(expand_exprs(values, macros, state, depth)?),
        ExprKind::Let { name, ty, value } => ExprKind::Let {
            name,
            ty: ty
                .map(|ty| expand_type(ty, macros, state, depth))
                .transpose()?,
            value: Box::new(expand_expr(*value, macros, state, depth)?),
        },
        ExprKind::Set { name, value } => ExprKind::Set {
            name,
            value: Box::new(expand_expr(*value, macros, state, depth)?),
        },
        ExprKind::Return(value) => ExprKind::Return(
            value
                .map(|value| expand_expr(*value, macros, state, depth).map(Box::new))
                .transpose()?,
        ),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => ExprKind::If {
            condition: Box::new(expand_expr(*condition, macros, state, depth)?),
            then_body: expand_exprs(then_body, macros, state, depth)?,
            else_body: expand_exprs(else_body, macros, state, depth)?,
        },
        ExprKind::While { condition, body } => ExprKind::While {
            condition: Box::new(expand_expr(*condition, macros, state, depth)?),
            body: expand_exprs(body, macros, state, depth)?,
        },
        ExprKind::For {
            binding,
            source,
            body,
        } => ExprKind::For {
            binding,
            source: Box::new(expand_expr(*source, macros, state, depth)?),
            body: expand_exprs(body, macros, state, depth)?,
        },
        ExprKind::Match { target, cases } => ExprKind::Match {
            target: Box::new(expand_expr(*target, macros, state, depth)?),
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.pattern = expand_pattern(case.pattern, macros, state, depth)?;
                    case.body = expand_exprs(case.body, macros, state, depth)?;
                    Ok(case)
                })
                .collect::<Result<_, _>>()?,
        },
        ExprKind::Record(fields) => ExprKind::Record(
            fields
                .into_iter()
                .map(|mut field| {
                    field.value = expand_expr(field.value, macros, state, depth)?;
                    Ok(field)
                })
                .collect::<Result<_, _>>()?,
        ),
        ExprKind::Tuple(values) => ExprKind::Tuple(expand_exprs(values, macros, state, depth)?),
        ExprKind::Array(values) => ExprKind::Array(expand_exprs(values, macros, state, depth)?),
        ExprKind::Map(entries) => ExprKind::Map(
            entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        expand_expr(key, macros, state, depth)?,
                        expand_expr(value, macros, state, depth)?,
                    ))
                })
                .collect::<Result<_, _>>()?,
        ),
        ExprKind::Mutable(value) => {
            ExprKind::Mutable(Box::new(expand_expr(*value, macros, state, depth)?))
        }
        ExprKind::ReferenceOf(value) => {
            ExprKind::ReferenceOf(Box::new(expand_expr(*value, macros, state, depth)?))
        }
        ExprKind::Range(start, end, step) => ExprKind::Range(
            Box::new(expand_expr(*start, macros, state, depth)?),
            Box::new(expand_expr(*end, macros, state, depth)?),
            Box::new(expand_expr(*step, macros, state, depth)?),
        ),
        ExprKind::Convert {
            value,
            into,
            fallback,
        } => ExprKind::Convert {
            value: Box::new(expand_expr(*value, macros, state, depth)?),
            into: expand_type(into, macros, state, depth)?,
            fallback,
        },
        ExprKind::Cast { value, into } => ExprKind::Cast {
            value: Box::new(expand_expr(*value, macros, state, depth)?),
            into: expand_type(into, macros, state, depth)?,
        },
        ExprKind::Task { captures, body } => ExprKind::Task {
            captures,
            body: expand_exprs(body, macros, state, depth)?,
        },
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => ExprKind::Spawn {
            handle,
            captures,
            value: Box::new(expand_expr(*value, macros, state, depth)?),
        },
        other => other,
    };
    Ok(expression)
}

fn expand_exprs(
    expressions: Vec<Expr>,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<Vec<Expr>, MacroExpansionError> {
    expressions
        .into_iter()
        .map(|expression| expand_expr(expression, macros, state, depth))
        .collect()
}

fn expand_type(
    mut ty: TypeExpr,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<TypeExpr, MacroExpansionError> {
    if let TypeExprKind::Application {
        constructor,
        arguments,
    } = &ty.value
    {
        if let Some(entry) = macros.get(&constructor.value) {
            let definition = &entry.definition;
            if entry.caller_alias.is_some() && definition.visibility == Visibility::Private {
                return Err(MacroExpansionError::at(
                    "E-MACRO-010",
                    format!("macro `{}` is private", constructor.value),
                    &ty.origin,
                )
                .related("private macro defined here", &definition.origin));
            }
            if depth >= TYPED_MACRO_MAX_EXPANSION_DEPTH {
                return Err(MacroExpansionError::at(
                    "E-MACRO-006",
                    "macro expansion depth exceeded",
                    &ty.origin,
                )
                .related("macro defined here", &definition.origin));
            }
            let fragment = invoke(
                definition,
                arguments
                    .iter()
                    .cloned()
                    .map(SyntaxFragment::Type)
                    .collect(),
                &ty.origin,
                state,
                entry.caller_alias.as_deref(),
                &entry.definition_symbols,
            )?;
            let SyntaxFragment::Type(mut expanded) = fragment else {
                return Err(category_error(definition, SyntaxCategory::Type, &ty.origin));
            };
            annotate_generated_type(&mut expanded, definition, &ty.origin, state)?;
            return expand_type(expanded, macros, state, depth + 1);
        }
    }
    ty.value = map_type_children(ty.value, |child| expand_type(child, macros, state, depth))?;
    Ok(ty)
}

fn map_type_children(
    value: TypeExprKind,
    mut map: impl FnMut(TypeExpr) -> Result<TypeExpr, MacroExpansionError>,
) -> Result<TypeExprKind, MacroExpansionError> {
    Ok(match value {
        TypeExprKind::Application {
            constructor,
            arguments,
        } => TypeExprKind::Application {
            constructor,
            arguments: arguments
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
        },
        TypeExprKind::Record(mut members) => {
            for member in &mut members {
                member.ty = map(member.ty.clone())?;
            }
            TypeExprKind::Record(members)
        }
        TypeExprKind::Enum(mut members) => {
            for member in &mut members {
                member.ty = map(member.ty.clone())?;
            }
            TypeExprKind::Enum(members)
        }
        TypeExprKind::Interface(mut members) => {
            for member in &mut members {
                member.ty = map(member.ty.clone())?;
            }
            TypeExprKind::Interface(members)
        }
        TypeExprKind::Tuple(values) => {
            TypeExprKind::Tuple(values.into_iter().map(&mut map).collect::<Result<_, _>>()?)
        }
        TypeExprKind::Array(value) => TypeExprKind::Array(Box::new(map(*value)?)),
        TypeExprKind::Map(key, value) => {
            TypeExprKind::Map(Box::new(map(*key)?), Box::new(map(*value)?))
        }
        TypeExprKind::Union(values) => {
            TypeExprKind::Union(values.into_iter().map(&mut map).collect::<Result<_, _>>()?)
        }
        TypeExprKind::Function { parameters, result } => TypeExprKind::Function {
            parameters: parameters
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
            result: Box::new(map(*result)?),
        },
        TypeExprKind::Newtype(value) => TypeExprKind::Newtype(Box::new(map(*value)?)),
        TypeExprKind::Mutable(value) => TypeExprKind::Mutable(Box::new(map(*value)?)),
        TypeExprKind::Reference(value) => TypeExprKind::Reference(Box::new(map(*value)?)),
        TypeExprKind::MutableReference(value) => {
            TypeExprKind::MutableReference(Box::new(map(*value)?))
        }
        TypeExprKind::Intersect(values) => {
            TypeExprKind::Intersect(values.into_iter().map(&mut map).collect::<Result<_, _>>()?)
        }
        TypeExprKind::Domain { head, arguments } => TypeExprKind::Domain {
            head,
            arguments: arguments
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
        },
        other => other,
    })
}

fn expand_pattern(
    mut pattern: Pattern,
    macros: &MacroTable,
    state: &mut ExpansionState,
    depth: usize,
) -> Result<Pattern, MacroExpansionError> {
    if let PatternKind::Constructor {
        constructor,
        arguments,
    } = &pattern.value
    {
        if let Some(entry) = macros.get(&constructor.value) {
            let definition = &entry.definition;
            if entry.caller_alias.is_some() && definition.visibility == Visibility::Private {
                return Err(MacroExpansionError::at(
                    "E-MACRO-010",
                    format!("macro `{}` is private", constructor.value),
                    &pattern.origin,
                )
                .related("private macro defined here", &definition.origin));
            }
            if depth >= TYPED_MACRO_MAX_EXPANSION_DEPTH {
                return Err(MacroExpansionError::at(
                    "E-MACRO-006",
                    "macro expansion depth exceeded",
                    &pattern.origin,
                )
                .related("macro defined here", &definition.origin));
            }
            let fragment = invoke(
                definition,
                arguments
                    .iter()
                    .cloned()
                    .map(SyntaxFragment::Pattern)
                    .collect(),
                &pattern.origin,
                state,
                entry.caller_alias.as_deref(),
                &entry.definition_symbols,
            )?;
            let SyntaxFragment::Pattern(mut expanded) = fragment else {
                return Err(category_error(
                    definition,
                    SyntaxCategory::Pattern,
                    &pattern.origin,
                ));
            };
            annotate_generated_pattern(&mut expanded, definition, &pattern.origin, state)?;
            return expand_pattern(expanded, macros, state, depth + 1);
        }
    }
    pattern.value = match pattern.value {
        PatternKind::Constructor {
            constructor,
            arguments,
        } => PatternKind::Constructor {
            constructor,
            arguments: arguments
                .into_iter()
                .map(|value| expand_pattern(value, macros, state, depth))
                .collect::<Result<_, _>>()?,
        },
        PatternKind::Record(fields) => PatternKind::Record(
            fields
                .into_iter()
                .map(|mut field| {
                    field.pattern = expand_pattern(field.pattern, macros, state, depth)?;
                    Ok(field)
                })
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Tuple(values) => PatternKind::Tuple(
            values
                .into_iter()
                .map(|value| expand_pattern(value, macros, state, depth))
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Array(values) => PatternKind::Array(
            values
                .into_iter()
                .map(|value| expand_pattern(value, macros, state, depth))
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Map(entries) => PatternKind::Map(
            entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        expand_pattern(key, macros, state, depth)?,
                        expand_pattern(value, macros, state, depth)?,
                    ))
                })
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Newtype { ty, pattern } => PatternKind::Newtype {
            ty: expand_type(ty, macros, state, depth)?,
            pattern: Box::new(expand_pattern(*pattern, macros, state, depth)?),
        },
        PatternKind::Interface { ty, pattern } => PatternKind::Interface {
            ty: expand_type(ty, macros, state, depth)?,
            pattern: Box::new(expand_pattern(*pattern, macros, state, depth)?),
        },
        other => other,
    };
    Ok(pattern)
}

fn invoke(
    definition: &Macro,
    arguments: Vec<SyntaxFragment>,
    call_origin: &Origin,
    state: &mut ExpansionState,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) -> Result<SyntaxFragment, MacroExpansionError> {
    if definition.parameters.len() != arguments.len() {
        return Err(MacroExpansionError::at(
            "E-MACRO-001",
            format!(
                "macro `{}` expects {} arguments but received {}",
                definition.name.value,
                definition.parameters.len(),
                arguments.len()
            ),
            call_origin,
        )
        .related("macro defined here", &definition.origin));
    }
    let mut environment = BTreeMap::new();
    for (parameter, argument) in definition.parameters.iter().zip(arguments) {
        if argument.category() != Some(parameter.category.value) {
            return Err(MacroExpansionError::at(
                "E-MACRO-002",
                format!(
                    "argument `{}` requires {:?} syntax",
                    parameter.name.value, parameter.category.value
                ),
                call_origin,
            )
            .related("parameter declared here", &parameter.name.origin));
        }
        environment.insert(parameter.name.value.clone(), argument);
    }
    let result = evaluate_body(
        &definition.body,
        &mut environment,
        definition,
        call_origin,
        state,
        caller_alias,
        definition_symbols,
    )?;
    if result.category() != Some(definition.result.value) {
        return Err(category_error(
            definition,
            definition.result.value,
            call_origin,
        ));
    }
    Ok(result)
}

fn evaluate_body(
    body: &[MacroExpr],
    environment: &mut BTreeMap<String, SyntaxFragment>,
    definition: &Macro,
    call_origin: &Origin,
    state: &mut ExpansionState,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) -> Result<SyntaxFragment, MacroExpansionError> {
    let mut result = None;
    for expression in body {
        step(state, &expression.origin)?;
        match &expression.value {
            MacroExprKind::Let { name, value } => {
                let value = evaluate(
                    value,
                    environment,
                    definition,
                    call_origin,
                    state,
                    caller_alias,
                    definition_symbols,
                )?;
                environment.insert(name.value.clone(), value);
            }
            _ => {
                result = Some(evaluate(
                    expression,
                    environment,
                    definition,
                    call_origin,
                    state,
                    caller_alias,
                    definition_symbols,
                )?);
            }
        }
    }
    result.ok_or_else(|| {
        MacroExpansionError::at(
            "E-MACRO-003",
            format!("macro `{}` produced no syntax", definition.name.value),
            &definition.origin,
        )
    })
}

fn evaluate(
    expression: &MacroExpr,
    environment: &mut BTreeMap<String, SyntaxFragment>,
    definition: &Macro,
    call_origin: &Origin,
    state: &mut ExpansionState,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) -> Result<SyntaxFragment, MacroExpansionError> {
    step(state, &expression.origin)?;
    match &expression.value {
        MacroExprKind::Atom(Atom::Bool(value)) => Ok(SyntaxFragment::Bool(*value)),
        MacroExprKind::Atom(Atom::Symbol(name)) => {
            environment.get(name).cloned().ok_or_else(|| {
                MacroExpansionError::at(
                    "E-MACRO-004",
                    format!("unknown compile-time binding `{name}`"),
                    &expression.origin,
                )
            })
        }
        MacroExprKind::Atom(_) => Err(MacroExpansionError::at(
            "E-MACRO-004",
            "macro atoms must be booleans or syntax bindings",
            &expression.origin,
        )),
        MacroExprKind::Let { .. } => Err(MacroExpansionError::at(
            "E-MACRO-004",
            "`let` is only valid as a macro body statement",
            &expression.origin,
        )),
        MacroExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            let condition = evaluate(
                condition,
                environment,
                definition,
                call_origin,
                state,
                caller_alias,
                definition_symbols,
            )?;
            let truth = match condition {
                SyntaxFragment::Bool(value) => value,
                SyntaxFragment::Expression(Spanned {
                    value: ExprKind::Literal(super::Literal::Bool(value)),
                    ..
                }) => value,
                _ => {
                    return Err(MacroExpansionError::at(
                        "E-MACRO-005",
                        "macro `if` condition must be boolean syntax",
                        &condition_origin(condition, expression),
                    ));
                }
            };
            evaluate_body(
                if truth { then_body } else { else_body },
                environment,
                definition,
                call_origin,
                state,
                caller_alias,
                definition_symbols,
            )
        }
        MacroExprKind::Quote { category, form } => {
            state.hygiene_id = state.hygiene_id.saturating_add(1);
            quote(
                category.value,
                form,
                environment,
                definition,
                call_origin,
                state.hygiene_id,
                caller_alias,
                definition_symbols,
            )
        }
        MacroExprKind::Unquote(name) => environment.get(&name.value).cloned().ok_or_else(|| {
            MacroExpansionError::at(
                "E-MACRO-004",
                format!("unknown unquote binding `{}`", name.value),
                &name.origin,
            )
        }),
        MacroExprKind::Splice(name) => Err(MacroExpansionError::at(
            "E-MACRO-007",
            format!(
                "splice `{}` is only valid inside a quoted sequence",
                name.value
            ),
            &name.origin,
        )),
        MacroExprKind::Capture(name) => Ok(SyntaxFragment::Expression(Spanned {
            value: ExprKind::Reference(name.value.clone()),
            span: name.span,
            origin: call_origin.clone(),
        })),
    }
}

fn condition_origin<'a>(_: SyntaxFragment, expression: &'a MacroExpr) -> Origin {
    expression.origin.clone()
}

fn quote(
    category: SyntaxCategory,
    form: &Node,
    environment: &BTreeMap<String, SyntaxFragment>,
    definition: &Macro,
    call_origin: &Origin,
    hygiene_id: usize,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) -> Result<SyntaxFragment, MacroExpansionError> {
    let document = definition
        .origin
        .document_id()
        .unwrap_or(DocumentId::ANONYMOUS);
    match category {
        SyntaxCategory::Expression => {
            let value = lower_expression_node_with_id(form, document)
                .map_err(|error| MacroExpansionError::from_ast(error, document))?;
            let mut value = substitute_expr(value, environment, call_origin)?;
            hygienize_expr(
                &mut value,
                document,
                definition.span,
                hygiene_id,
                &mut BTreeMap::new(),
                caller_alias,
                definition_symbols,
            );
            Ok(SyntaxFragment::Expression(value))
        }
        SyntaxCategory::Type => {
            let value = lower_type_node_with_id(form, document)
                .map_err(|error| MacroExpansionError::from_ast(error, document))?;
            let mut value = substitute_type(value, environment)?;
            qualify_generated_type_names(
                &mut value,
                document,
                definition.span,
                caller_alias,
                definition_symbols,
            );
            Ok(SyntaxFragment::Type(value))
        }
        SyntaxCategory::Pattern => {
            let value = lower_pattern_node_with_id(form, document)
                .map_err(|error| MacroExpansionError::from_ast(error, document))?;
            let mut value = substitute_pattern(value, environment)?;
            hygienize_pattern_bindings(
                &mut value,
                document,
                definition.span,
                hygiene_id,
                &mut BTreeMap::new(),
            );
            qualify_generated_pattern_names(
                &mut value,
                document,
                definition.span,
                caller_alias,
                definition_symbols,
            );
            Ok(SyntaxFragment::Pattern(value))
        }
        SyntaxCategory::Definition => {
            let value = lower_top_level_node_with_id(form, document)
                .map_err(|error| MacroExpansionError::from_ast(error, document))?;
            Ok(SyntaxFragment::Definition(value))
        }
        SyntaxCategory::Module => {
            let value = lower_top_level_node_with_id(form, document)
                .map_err(|error| MacroExpansionError::from_ast(error, document))?;
            Ok(SyntaxFragment::Module(vec![value]))
        }
    }
}

fn substitute_expr(
    mut expression: Expr,
    environment: &BTreeMap<String, SyntaxFragment>,
    call_origin: &Origin,
) -> Result<Expr, MacroExpansionError> {
    if let ExprKind::Call { callee, arguments } = &expression.value {
        if callee.value == "unquote" && arguments.len() == 1 {
            let name = expression_reference(&arguments[0]).ok_or_else(|| {
                MacroExpansionError::at(
                    "E-MACRO-007",
                    "unquote requires one binding name",
                    &expression.origin,
                )
            })?;
            return match environment.get(name) {
                Some(SyntaxFragment::Expression(value)) => Ok(value.clone()),
                Some(_) => Err(MacroExpansionError::at(
                    "E-MACRO-002",
                    format!("unquote `{name}` is not expression syntax"),
                    &expression.origin,
                )),
                None => Err(MacroExpansionError::at(
                    "E-MACRO-004",
                    format!("unknown unquote binding `{name}`"),
                    &expression.origin,
                )),
            };
        }
        if callee.value == "capture" && arguments.len() == 1 {
            let name = expression_reference(&arguments[0]).ok_or_else(|| {
                MacroExpansionError::at(
                    "E-MACRO-007",
                    "capture requires one binding name",
                    &expression.origin,
                )
            })?;
            return Ok(Spanned {
                value: ExprKind::Reference(name.to_string()),
                span: expression.span,
                origin: call_origin.clone(),
            });
        }
        if callee.value == "splice" {
            return Err(MacroExpansionError::at(
                "E-MACRO-007",
                "splice is only valid as an item in a quoted expression sequence",
                &expression.origin,
            ));
        }
    }
    expression.value = substitute_expr_kind(expression.value, environment, call_origin)?;
    Ok(expression)
}

fn substitute_expr_kind(
    value: ExprKind,
    environment: &BTreeMap<String, SyntaxFragment>,
    call_origin: &Origin,
) -> Result<ExprKind, MacroExpansionError> {
    let one = |value| substitute_expr(value, environment, call_origin);
    Ok(match value {
        ExprKind::Call { callee, arguments } => ExprKind::Call {
            callee,
            arguments: substitute_exprs(arguments, environment, call_origin)?,
        },
        ExprKind::Do(values) => ExprKind::Do(substitute_exprs(values, environment, call_origin)?),
        ExprKind::Let { name, ty, value } => ExprKind::Let {
            name,
            ty: ty.map(|ty| substitute_type(ty, environment)).transpose()?,
            value: Box::new(one(*value)?),
        },
        ExprKind::Set { name, value } => ExprKind::Set {
            name,
            value: Box::new(one(*value)?),
        },
        ExprKind::Return(value) => {
            ExprKind::Return(value.map(|value| one(*value).map(Box::new)).transpose()?)
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => ExprKind::If {
            condition: Box::new(one(*condition)?),
            then_body: substitute_exprs(then_body, environment, call_origin)?,
            else_body: substitute_exprs(else_body, environment, call_origin)?,
        },
        ExprKind::While { condition, body } => ExprKind::While {
            condition: Box::new(one(*condition)?),
            body: substitute_exprs(body, environment, call_origin)?,
        },
        ExprKind::For {
            binding,
            source,
            body,
        } => ExprKind::For {
            binding,
            source: Box::new(one(*source)?),
            body: substitute_exprs(body, environment, call_origin)?,
        },
        ExprKind::Match { target, cases } => ExprKind::Match {
            target: Box::new(one(*target)?),
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.pattern = substitute_pattern(case.pattern, environment)?;
                    case.body = substitute_exprs(case.body, environment, call_origin)?;
                    Ok(case)
                })
                .collect::<Result<_, _>>()?,
        },
        ExprKind::Record(fields) => ExprKind::Record(
            fields
                .into_iter()
                .map(|mut field| {
                    field.value = one(field.value)?;
                    Ok(field)
                })
                .collect::<Result<_, _>>()?,
        ),
        ExprKind::Tuple(values) => {
            ExprKind::Tuple(substitute_exprs(values, environment, call_origin)?)
        }
        ExprKind::Array(values) => {
            ExprKind::Array(substitute_exprs(values, environment, call_origin)?)
        }
        ExprKind::Map(entries) => ExprKind::Map(
            entries
                .into_iter()
                .map(|(key, value)| Ok((one(key)?, one(value)?)))
                .collect::<Result<_, _>>()?,
        ),
        ExprKind::Mutable(value) => ExprKind::Mutable(Box::new(one(*value)?)),
        ExprKind::ReferenceOf(value) => ExprKind::ReferenceOf(Box::new(one(*value)?)),
        ExprKind::Range(start, end, step) => ExprKind::Range(
            Box::new(one(*start)?),
            Box::new(one(*end)?),
            Box::new(one(*step)?),
        ),
        ExprKind::Convert {
            value,
            into,
            fallback,
        } => ExprKind::Convert {
            value: Box::new(one(*value)?),
            into: substitute_type(into, environment)?,
            fallback,
        },
        ExprKind::Cast { value, into } => ExprKind::Cast {
            value: Box::new(one(*value)?),
            into: substitute_type(into, environment)?,
        },
        ExprKind::Task { captures, body } => ExprKind::Task {
            captures,
            body: substitute_exprs(body, environment, call_origin)?,
        },
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => ExprKind::Spawn {
            handle,
            captures,
            value: Box::new(one(*value)?),
        },
        other => other,
    })
}

fn substitute_exprs(
    values: Vec<Expr>,
    environment: &BTreeMap<String, SyntaxFragment>,
    call_origin: &Origin,
) -> Result<Vec<Expr>, MacroExpansionError> {
    let mut output = Vec::new();
    for value in values {
        if let Some(name) = splice_name(&value) {
            match environment.get(name) {
                Some(SyntaxFragment::Expression(Spanned {
                    value: ExprKind::Do(values),
                    ..
                })) => output.extend(values.clone()),
                Some(SyntaxFragment::Expression(value)) => output.push(value.clone()),
                Some(_) => {
                    return Err(MacroExpansionError::at(
                        "E-MACRO-002",
                        format!("splice `{name}` is not expression syntax"),
                        &value.origin,
                    ));
                }
                None => {
                    return Err(MacroExpansionError::at(
                        "E-MACRO-004",
                        format!("unknown splice binding `{name}`"),
                        &value.origin,
                    ));
                }
            }
        } else {
            output.push(substitute_expr(value, environment, call_origin)?);
        }
    }
    Ok(output)
}

fn splice_name(expression: &Expr) -> Option<&str> {
    let ExprKind::Call { callee, arguments } = &expression.value else {
        return None;
    };
    (callee.value == "splice" && arguments.len() == 1)
        .then(|| expression_reference(&arguments[0]))
        .flatten()
}

fn expression_reference(expression: &Expr) -> Option<&str> {
    match &expression.value {
        ExprKind::Reference(name) => Some(name),
        _ => None,
    }
}

fn substitute_type(
    mut ty: TypeExpr,
    environment: &BTreeMap<String, SyntaxFragment>,
) -> Result<TypeExpr, MacroExpansionError> {
    if let TypeExprKind::Application {
        constructor,
        arguments,
    } = &ty.value
    {
        if constructor.value == "unquote" && arguments.len() == 1 {
            let TypeExprKind::Named(name) = &arguments[0].value else {
                return Err(MacroExpansionError::at(
                    "E-MACRO-007",
                    "type unquote requires one binding name",
                    &ty.origin,
                ));
            };
            return match environment.get(name) {
                Some(SyntaxFragment::Type(value)) => Ok(value.clone()),
                Some(_) => Err(MacroExpansionError::at(
                    "E-MACRO-002",
                    format!("unquote `{name}` is not type syntax"),
                    &ty.origin,
                )),
                None => Err(MacroExpansionError::at(
                    "E-MACRO-004",
                    format!("unknown unquote binding `{name}`"),
                    &ty.origin,
                )),
            };
        }
    }
    ty.value = map_type_children(ty.value, |child| substitute_type(child, environment))?;
    Ok(ty)
}

fn substitute_pattern(
    mut pattern: Pattern,
    environment: &BTreeMap<String, SyntaxFragment>,
) -> Result<Pattern, MacroExpansionError> {
    if let PatternKind::Constructor {
        constructor,
        arguments,
    } = &pattern.value
    {
        if constructor.value == "unquote" && arguments.len() == 1 {
            let PatternKind::Bind(name) = &arguments[0].value else {
                return Err(MacroExpansionError::at(
                    "E-MACRO-007",
                    "pattern unquote requires one binding name",
                    &pattern.origin,
                ));
            };
            return match environment.get(&name.value) {
                Some(SyntaxFragment::Pattern(value)) => Ok(value.clone()),
                Some(_) => Err(MacroExpansionError::at(
                    "E-MACRO-002",
                    format!("unquote `{}` is not pattern syntax", name.value),
                    &pattern.origin,
                )),
                None => Err(MacroExpansionError::at(
                    "E-MACRO-004",
                    format!("unknown unquote binding `{}`", name.value),
                    &pattern.origin,
                )),
            };
        }
    }
    pattern.value = match pattern.value {
        PatternKind::Constructor {
            constructor,
            arguments,
        } => PatternKind::Constructor {
            constructor,
            arguments: arguments
                .into_iter()
                .map(|pattern| substitute_pattern(pattern, environment))
                .collect::<Result<_, _>>()?,
        },
        PatternKind::Record(fields) => PatternKind::Record(
            fields
                .into_iter()
                .map(|mut field| {
                    field.pattern = substitute_pattern(field.pattern, environment)?;
                    Ok(field)
                })
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Tuple(values) => PatternKind::Tuple(
            values
                .into_iter()
                .map(|pattern| substitute_pattern(pattern, environment))
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Array(values) => PatternKind::Array(
            values
                .into_iter()
                .map(|pattern| substitute_pattern(pattern, environment))
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Map(entries) => PatternKind::Map(
            entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        substitute_pattern(key, environment)?,
                        substitute_pattern(value, environment)?,
                    ))
                })
                .collect::<Result<_, _>>()?,
        ),
        PatternKind::Newtype { ty, pattern } => PatternKind::Newtype {
            ty: substitute_type(ty, environment)?,
            pattern: Box::new(substitute_pattern(*pattern, environment)?),
        },
        PatternKind::Interface { ty, pattern } => PatternKind::Interface {
            ty: substitute_type(ty, environment)?,
            pattern: Box::new(substitute_pattern(*pattern, environment)?),
        },
        other => other,
    };
    Ok(pattern)
}

fn hygienize_expr(
    expression: &mut Expr,
    definition_document: DocumentId,
    definition_span: Span,
    hygiene_id: usize,
    bindings: &mut BTreeMap<String, String>,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) {
    let source_span = expression.origin.primary_span();
    let generated = expression.origin.document_id() == Some(definition_document)
        && definition_span.start <= source_span.start
        && source_span.end <= definition_span.end;
    match &mut expression.value {
        ExprKind::Reference(name) if generated => {
            if let Some(replacement) = bindings.get(name) {
                *name = replacement.clone();
            } else {
                qualify_definition_reference(name, caller_alias, definition_symbols);
            }
        }
        ExprKind::Let { name, ty, value } => {
            if let Some(ty) = ty {
                qualify_generated_type_names(
                    ty,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
            hygienize_expr(
                value,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            if generated {
                let original = name.value.clone();
                let replacement = format!("{original}--macro-{hygiene_id}");
                name.value = replacement.clone();
                bindings.insert(original, replacement);
            }
        }
        ExprKind::Call { callee, arguments } => {
            if generated {
                if let Some(replacement) = bindings.get(&callee.value) {
                    callee.value = replacement.clone();
                } else {
                    qualify_definition_reference(
                        &mut callee.value,
                        caller_alias,
                        definition_symbols,
                    );
                }
            }
            for value in arguments {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Do(arguments) => {
            for value in arguments {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Tuple(arguments) | ExprKind::Array(arguments) => {
            for value in arguments {
                let mut argument_scope = bindings.clone();
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut argument_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            hygienize_expr(
                condition,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            let mut then_scope = bindings.clone();
            for value in then_body {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut then_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
            let mut else_scope = bindings.clone();
            for value in else_body {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut else_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::While { condition, body } => {
            hygienize_expr(
                condition,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            let mut body_scope = bindings.clone();
            for value in body {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut body_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::For {
            binding,
            source,
            body,
        } => {
            hygienize_expr(
                source,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            let mut body_scope = bindings.clone();
            rename_binding(binding, generated, hygiene_id, &mut body_scope);
            for value in body {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut body_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Match { target, cases } => {
            hygienize_expr(
                target,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            for case in cases {
                let mut case_scope = bindings.clone();
                hygienize_pattern_bindings(
                    &mut case.pattern,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut case_scope,
                );
                qualify_generated_pattern_names(
                    &mut case.pattern,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
                for value in &mut case.body {
                    hygienize_expr(
                        value,
                        definition_document,
                        definition_span,
                        hygiene_id,
                        &mut case_scope,
                        caller_alias,
                        definition_symbols,
                    );
                }
            }
        }
        ExprKind::Record(fields) => {
            for field in fields {
                let mut field_scope = bindings.clone();
                hygienize_expr(
                    &mut field.value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut field_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                for item in [key, value] {
                    let mut item_scope = bindings.clone();
                    hygienize_expr(
                        item,
                        definition_document,
                        definition_span,
                        hygiene_id,
                        &mut item_scope,
                        caller_alias,
                        definition_symbols,
                    );
                }
            }
        }
        ExprKind::Set { name, value } => {
            if generated {
                if let Some(replacement) = bindings.get(&name.value) {
                    name.value = replacement.clone();
                }
            }
            hygienize_expr(
                value,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
        }
        ExprKind::Return(Some(value)) | ExprKind::Mutable(value) | ExprKind::ReferenceOf(value) => {
            hygienize_expr(
                value,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            )
        }
        ExprKind::Range(start, end, step) => {
            for value in [start, end, step] {
                let mut value_scope = bindings.clone();
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut value_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Convert { value, into, .. } | ExprKind::Cast { value, into } => {
            hygienize_expr(
                value,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            qualify_generated_type_names(
                into,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
        }
        ExprKind::Task { captures, body } => {
            for capture in captures {
                if generated {
                    if let Some(replacement) = bindings.get(&capture.value) {
                        capture.value = replacement.clone();
                    }
                }
            }
            let mut body_scope = bindings.clone();
            for value in body {
                hygienize_expr(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    &mut body_scope,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => {
            for capture in captures {
                if generated {
                    if let Some(replacement) = bindings.get(&capture.value) {
                        capture.value = replacement.clone();
                    }
                }
            }
            hygienize_expr(
                value,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
                caller_alias,
                definition_symbols,
            );
            rename_binding(handle, generated, hygiene_id, bindings);
        }
        ExprKind::Join { handle, binding } => {
            if generated {
                if let Some(replacement) = bindings.get(&handle.value) {
                    handle.value = replacement.clone();
                }
            }
            rename_binding(binding, generated, hygiene_id, bindings);
        }
        _ => {}
    }
}

fn rename_binding(
    name: &mut super::Name,
    generated: bool,
    hygiene_id: usize,
    bindings: &mut BTreeMap<String, String>,
) {
    if generated {
        let original = name.value.clone();
        let replacement = format!("{original}--macro-{hygiene_id}");
        name.value = replacement.clone();
        bindings.insert(original, replacement);
    }
}

fn hygienize_pattern_bindings(
    pattern: &mut Pattern,
    definition_document: DocumentId,
    definition_span: Span,
    hygiene_id: usize,
    bindings: &mut BTreeMap<String, String>,
) {
    let span = pattern.origin.primary_span();
    let generated = pattern.origin.document_id() == Some(definition_document)
        && definition_span.start <= span.start
        && span.end <= definition_span.end;
    match &mut pattern.value {
        PatternKind::Bind(name) => rename_binding(name, generated, hygiene_id, bindings),
        PatternKind::Constructor { arguments, .. }
        | PatternKind::Tuple(arguments)
        | PatternKind::Array(arguments) => {
            for pattern in arguments {
                hygienize_pattern_bindings(
                    pattern,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                );
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                hygienize_pattern_bindings(
                    &mut field.pattern,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                );
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                hygienize_pattern_bindings(
                    key,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                );
                hygienize_pattern_bindings(
                    value,
                    definition_document,
                    definition_span,
                    hygiene_id,
                    bindings,
                );
            }
        }
        PatternKind::Newtype { pattern, .. } | PatternKind::Interface { pattern, .. } => {
            hygienize_pattern_bindings(
                pattern,
                definition_document,
                definition_span,
                hygiene_id,
                bindings,
            );
        }
        PatternKind::Literal(_) | PatternKind::Wildcard => {}
    }
}

fn qualify_generated_type_names(
    ty: &mut TypeExpr,
    definition_document: DocumentId,
    definition_span: Span,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) {
    let span = ty.origin.primary_span();
    let generated = ty.origin.document_id() == Some(definition_document)
        && definition_span.start <= span.start
        && span.end <= definition_span.end;
    match &mut ty.value {
        TypeExprKind::Named(name) if generated => {
            qualify_definition_reference(name, caller_alias, definition_symbols);
        }
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            if generated {
                qualify_definition_reference(
                    &mut constructor.value,
                    caller_alias,
                    definition_symbols,
                );
            }
            for argument in arguments {
                qualify_generated_type_names(
                    argument,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                qualify_generated_type_names(
                    &mut member.ty,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        TypeExprKind::Tuple(values)
        | TypeExprKind::Union(values)
        | TypeExprKind::Intersect(values) => {
            for value in values {
                qualify_generated_type_names(
                    value,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        TypeExprKind::Array(value)
        | TypeExprKind::Newtype(value)
        | TypeExprKind::Mutable(value)
        | TypeExprKind::Reference(value)
        | TypeExprKind::MutableReference(value) => qualify_generated_type_names(
            value,
            definition_document,
            definition_span,
            caller_alias,
            definition_symbols,
        ),
        TypeExprKind::Map(key, value) => {
            qualify_generated_type_names(
                key,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
            qualify_generated_type_names(
                value,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
        }
        TypeExprKind::Function { parameters, result } => {
            for parameter in parameters {
                qualify_generated_type_names(
                    parameter,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
            qualify_generated_type_names(
                result,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
        }
        TypeExprKind::Domain { head, arguments } => {
            if generated {
                qualify_definition_reference(&mut head.value, caller_alias, definition_symbols);
            }
            for argument in arguments {
                qualify_generated_type_names(
                    argument,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        TypeExprKind::Named(_) => {}
    }
}

fn qualify_generated_pattern_names(
    pattern: &mut Pattern,
    definition_document: DocumentId,
    definition_span: Span,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) {
    let span = pattern.origin.primary_span();
    let generated = pattern.origin.document_id() == Some(definition_document)
        && definition_span.start <= span.start
        && span.end <= definition_span.end;
    match &mut pattern.value {
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            if generated {
                qualify_definition_reference(
                    &mut constructor.value,
                    caller_alias,
                    definition_symbols,
                );
            }
            for argument in arguments {
                qualify_generated_pattern_names(
                    argument,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                qualify_generated_pattern_names(
                    &mut field.pattern,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        PatternKind::Tuple(values) | PatternKind::Array(values) => {
            for value in values {
                qualify_generated_pattern_names(
                    value,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                qualify_generated_pattern_names(
                    key,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
                qualify_generated_pattern_names(
                    value,
                    definition_document,
                    definition_span,
                    caller_alias,
                    definition_symbols,
                );
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            qualify_generated_type_names(
                ty,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
            qualify_generated_pattern_names(
                pattern,
                definition_document,
                definition_span,
                caller_alias,
                definition_symbols,
            );
        }
        PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Bind(_) => {}
    }
}

fn qualify_definition_reference(
    reference: &mut String,
    caller_alias: Option<&str>,
    definition_symbols: &BTreeSet<String>,
) {
    let Some(alias) = caller_alias else {
        return;
    };
    let root = reference.split('.').next().unwrap_or(reference);
    if !reference.contains('.') && definition_symbols.contains(root) {
        *reference = format!("{alias}.{reference}");
    }
}

fn annotate_generated_expr(
    expression: &mut Expr,
    definition: &Macro,
    call: &Origin,
    state: &mut ExpansionState,
) -> Result<(), MacroExpansionError> {
    annotate_generated_origin(&mut expression.origin, definition, call, state)?;
    match &mut expression.value {
        ExprKind::Call { callee, arguments } => {
            annotate_generated_name(callee, definition, call, state)?;
            for value in arguments {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::Do(values) | ExprKind::Tuple(values) | ExprKind::Array(values) => {
            for value in values {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::Let { name, ty, value } => {
            annotate_generated_name(name, definition, call, state)?;
            if let Some(ty) = ty {
                annotate_generated_type(ty, definition, call, state)?;
            }
            annotate_generated_expr(value, definition, call, state)?;
        }
        ExprKind::Set { name, value } => {
            annotate_generated_name(name, definition, call, state)?;
            annotate_generated_expr(value, definition, call, state)?;
        }
        ExprKind::Return(Some(value)) | ExprKind::Mutable(value) | ExprKind::ReferenceOf(value) => {
            annotate_generated_expr(value, definition, call, state)?;
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            annotate_generated_expr(condition, definition, call, state)?;
            for value in then_body.iter_mut().chain(else_body) {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::While { condition, body } => {
            annotate_generated_expr(condition, definition, call, state)?;
            for value in body {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::For {
            binding,
            source,
            body,
        } => {
            annotate_generated_name(binding, definition, call, state)?;
            annotate_generated_expr(source, definition, call, state)?;
            for value in body {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::Match { target, cases } => {
            annotate_generated_expr(target, definition, call, state)?;
            for case in cases {
                annotate_generated_pattern(&mut case.pattern, definition, call, state)?;
                for value in &mut case.body {
                    annotate_generated_expr(value, definition, call, state)?;
                }
            }
        }
        ExprKind::Record(fields) => {
            for field in fields {
                annotate_generated_name(&mut field.name, definition, call, state)?;
                annotate_generated_expr(&mut field.value, definition, call, state)?;
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                annotate_generated_expr(key, definition, call, state)?;
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::Range(start, end, step) => {
            annotate_generated_expr(start, definition, call, state)?;
            annotate_generated_expr(end, definition, call, state)?;
            annotate_generated_expr(step, definition, call, state)?;
        }
        ExprKind::Convert { value, into, .. } | ExprKind::Cast { value, into } => {
            annotate_generated_expr(value, definition, call, state)?;
            annotate_generated_type(into, definition, call, state)?;
        }
        ExprKind::Task { captures, body } => {
            for capture in captures {
                annotate_generated_name(capture, definition, call, state)?;
            }
            for value in body {
                annotate_generated_expr(value, definition, call, state)?;
            }
        }
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => {
            annotate_generated_name(handle, definition, call, state)?;
            for capture in captures {
                annotate_generated_name(capture, definition, call, state)?;
            }
            annotate_generated_expr(value, definition, call, state)?;
        }
        ExprKind::Join { handle, binding } => {
            annotate_generated_name(handle, definition, call, state)?;
            annotate_generated_name(binding, definition, call, state)?;
        }
        ExprKind::Literal(_)
        | ExprKind::Reference(_)
        | ExprKind::Return(None)
        | ExprKind::Break
        | ExprKind::Continue => {}
    }
    Ok(())
}

fn annotate_generated_type(
    ty: &mut TypeExpr,
    definition: &Macro,
    call: &Origin,
    state: &mut ExpansionState,
) -> Result<(), MacroExpansionError> {
    annotate_generated_origin(&mut ty.origin, definition, call, state)?;
    match &mut ty.value {
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            annotate_generated_name(constructor, definition, call, state)?;
            for argument in arguments {
                annotate_generated_type(argument, definition, call, state)?;
            }
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                annotate_generated_name(&mut member.name, definition, call, state)?;
                annotate_generated_type(&mut member.ty, definition, call, state)?;
            }
        }
        TypeExprKind::Tuple(values)
        | TypeExprKind::Union(values)
        | TypeExprKind::Intersect(values) => {
            for value in values {
                annotate_generated_type(value, definition, call, state)?;
            }
        }
        TypeExprKind::Array(value)
        | TypeExprKind::Newtype(value)
        | TypeExprKind::Mutable(value)
        | TypeExprKind::Reference(value)
        | TypeExprKind::MutableReference(value) => {
            annotate_generated_type(value, definition, call, state)?;
        }
        TypeExprKind::Map(key, value) => {
            annotate_generated_type(key, definition, call, state)?;
            annotate_generated_type(value, definition, call, state)?;
        }
        TypeExprKind::Function { parameters, result } => {
            for parameter in parameters {
                annotate_generated_type(parameter, definition, call, state)?;
            }
            annotate_generated_type(result, definition, call, state)?;
        }
        TypeExprKind::Domain { head, arguments } => {
            annotate_generated_name(head, definition, call, state)?;
            for argument in arguments {
                annotate_generated_type(argument, definition, call, state)?;
            }
        }
        TypeExprKind::Named(_) => {}
    }
    Ok(())
}

fn annotate_generated_pattern(
    pattern: &mut Pattern,
    definition: &Macro,
    call: &Origin,
    state: &mut ExpansionState,
) -> Result<(), MacroExpansionError> {
    annotate_generated_origin(&mut pattern.origin, definition, call, state)?;
    match &mut pattern.value {
        PatternKind::Bind(name) => annotate_generated_name(name, definition, call, state)?,
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            annotate_generated_name(constructor, definition, call, state)?;
            for argument in arguments {
                annotate_generated_pattern(argument, definition, call, state)?;
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                annotate_generated_name(&mut field.name, definition, call, state)?;
                annotate_generated_pattern(&mut field.pattern, definition, call, state)?;
            }
        }
        PatternKind::Tuple(values) | PatternKind::Array(values) => {
            for value in values {
                annotate_generated_pattern(value, definition, call, state)?;
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                annotate_generated_pattern(key, definition, call, state)?;
                annotate_generated_pattern(value, definition, call, state)?;
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            annotate_generated_type(ty, definition, call, state)?;
            annotate_generated_pattern(pattern, definition, call, state)?;
        }
        PatternKind::Literal(_) | PatternKind::Wildcard => {}
    }
    Ok(())
}

fn annotate_generated_name(
    name: &mut super::Name,
    definition: &Macro,
    call: &Origin,
    state: &mut ExpansionState,
) -> Result<(), MacroExpansionError> {
    annotate_generated_origin(&mut name.origin, definition, call, state)
}

fn annotate_generated_origin(
    origin: &mut Origin,
    definition: &Macro,
    call: &Origin,
    state: &mut ExpansionState,
) -> Result<(), MacroExpansionError> {
    let span = origin.primary_span();
    let definition_owned = origin.document_id() == definition.origin.document_id()
        && definition.span.start <= span.start
        && span.end <= definition.span.end;
    if definition_owned {
        let ordinal = generated(state, call)?;
        *origin = expansion_origin(call, &definition.origin, origin.clone(), ordinal);
    }
    Ok(())
}

fn expansion_origin(call: &Origin, definition: &Origin, parent: Origin, ordinal: u64) -> Origin {
    match (call.document_id(), definition.document_id()) {
        (Some(call_document), Some(definition_document)) => Origin::DocumentExpansion {
            ast_id: super::AstId::from_raw(stable_expansion_id(
                call_document,
                call.primary_span(),
                definition_document,
                definition.primary_span(),
                ordinal,
            )),
            call_site: SourceLocation {
                document: call_document,
                span: call.primary_span(),
            },
            definition: SourceLocation {
                document: definition_document,
                span: definition.primary_span(),
            },
            parent: std::sync::Arc::new(parent),
        },
        _ => Origin::Expansion {
            call_site: call.primary_span(),
            definition: definition.primary_span(),
            parent: std::sync::Arc::new(parent),
        },
    }
}

fn stable_expansion_id(
    call_document: DocumentId,
    call_span: Span,
    definition_document: DocumentId,
    definition_span: Span,
    ordinal: u64,
) -> u64 {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in call_document
        .raw()
        .to_le_bytes()
        .into_iter()
        .chain((call_span.start as u64).to_le_bytes())
        .chain((call_span.end as u64).to_le_bytes())
        .chain(definition_document.raw().to_le_bytes())
        .chain((definition_span.start as u64).to_le_bytes())
        .chain((definition_span.end as u64).to_le_bytes())
        .chain(ordinal.to_le_bytes())
    {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    value
}

fn category_error(
    definition: &Macro,
    expected: SyntaxCategory,
    call_origin: &Origin,
) -> MacroExpansionError {
    MacroExpansionError::at(
        "E-MACRO-002",
        format!(
            "macro `{}` returns {:?} syntax but is used as {:?} syntax",
            definition.name.value, definition.result.value, expected
        ),
        call_origin,
    )
    .related("macro declared here", &definition.result.origin)
}

fn step(state: &mut ExpansionState, origin: &Origin) -> Result<(), MacroExpansionError> {
    state.evaluation_steps = state.evaluation_steps.saturating_add(1);
    if state.evaluation_steps > TYPED_MACRO_MAX_EVALUATION_STEPS {
        return Err(MacroExpansionError::at(
            "E-MACRO-008",
            format!(
                "macro evaluation exceeded {} steps",
                TYPED_MACRO_MAX_EVALUATION_STEPS
            ),
            origin,
        ));
    }
    Ok(())
}

fn generated(state: &mut ExpansionState, origin: &Origin) -> Result<u64, MacroExpansionError> {
    state.generated_nodes = state.generated_nodes.saturating_add(1);
    if state.generated_nodes > TYPED_MACRO_MAX_GENERATED_NODES {
        return Err(MacroExpansionError::at(
            "E-MACRO-009",
            format!(
                "macro expansion generated more than {} nodes",
                TYPED_MACRO_MAX_GENERATED_NODES
            ),
            origin,
        ));
    }
    Ok(state.generated_nodes as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ast::lower_document_with_id, syntax};

    fn module(source: &str, id: u64) -> Module {
        let document = syntax::parse(source).unwrap();
        lower_document_with_id(&document, DocumentId::from_raw(id)).unwrap()
    }

    #[test]
    fn expands_expression_macros_with_hygiene_and_document_origins() {
        let source = "
(macro twice ((value expr-syntax)) expr-syntax
  (do (quote expr-syntax
    (do (let temporary (unquote value)) temporary))))
(fn main ((temporary int64)) int64 (do (twice temporary)))";
        let expanded = expand_typed_macros(module(source, 10)).unwrap();
        assert_eq!(expanded.forms.len(), 1);
        let TopLevel::Function(function) = &expanded.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Do(values) = &function.body[0].value else {
            panic!("expected quoted do");
        };
        let ExprKind::Let { name, value, .. } = &values[0].value else {
            panic!("expected hygienic let");
        };
        assert_eq!(name.value, "temporary--macro-1");
        assert!(matches!(
            value.value,
            ExprKind::Reference(ref name) if name == "temporary"
        ));
        let Origin::DocumentExpansion {
            call_site,
            definition,
            parent,
            ..
        } = &function.body[0].origin
        else {
            panic!("expected document-qualified expansion origin");
        };
        assert_eq!(call_site.document, DocumentId::from_raw(10));
        assert_eq!(definition.document, DocumentId::from_raw(10));
        assert_eq!(parent.document_id(), Some(DocumentId::from_raw(10)));
        let generated_ids = [
            function.body[0].origin.ast_id().unwrap(),
            values[0].origin.ast_id().unwrap(),
            values[1].origin.ast_id().unwrap(),
            name.origin.ast_id().unwrap(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(generated_ids.len(), 4);
        assert!(matches!(values[0].origin, Origin::DocumentExpansion { .. }));
        assert!(matches!(values[1].origin, Origin::DocumentExpansion { .. }));
    }

    #[test]
    fn rejects_parameter_and_result_category_mismatches() {
        let source = "
(macro as-type ((value type-syntax)) type-syntax
  (do (unquote value)))
(fn main () int64 (do (as-type 1)))";
        let error = expand_typed_macros(module(source, 1)).unwrap_err();
        assert_eq!(error.code, "E-MACRO-002");
        assert!(!error.related.is_empty());
    }

    #[test]
    fn expands_type_category_without_an_untyped_bridge() {
        let source = "
(macro optional ((value type-syntax)) type-syntax
  (do (quote type-syntax (option (unquote value)))))
(def maybe-int (optional int64))";
        let expanded = expand_typed_macros(module(source, 7)).unwrap();
        let TopLevel::Definition(definition) = &expanded.forms[0] else {
            panic!("expected definition");
        };
        let TypeExprKind::Application {
            constructor,
            arguments,
        } = &definition.body.value
        else {
            panic!("expected expanded type application");
        };
        assert_eq!(constructor.value, "option");
        assert!(matches!(
            arguments[0].value,
            TypeExprKind::Named(ref name) if name == "int64"
        ));
        assert!(matches!(
            definition.body.origin,
            Origin::DocumentExpansion { .. }
        ));
    }

    #[test]
    fn expands_and_annotates_mutable_reference_types() {
        let source = "
(macro exclusive ((value type-syntax)) type-syntax
  (do (quote type-syntax (mut-ref (unquote value)))))
(def exclusive-int (exclusive int64))";
        let expanded = expand_typed_macros(module(source, 8)).unwrap();
        let TopLevel::Definition(definition) = &expanded.forms[0] else {
            panic!("expected definition");
        };
        let TypeExprKind::MutableReference(inner) = &definition.body.value else {
            panic!("expected expanded mutable reference");
        };
        assert!(matches!(
            inner.value,
            TypeExprKind::Named(ref name) if name == "int64"
        ));
        assert!(matches!(
            definition.body.origin,
            Origin::DocumentExpansion { .. }
        ));
        assert_eq!(inner.origin.document_id(), Some(DocumentId::from_raw(8)));
    }

    #[test]
    fn rejects_splice_outside_a_quote_sequence() {
        let source = "
(macro invalid ((value expr-syntax)) expr-syntax
  (do (splice value)))
(fn main () int64 (do (invalid 1)))";
        let error = expand_typed_macros(module(source, 1)).unwrap_err();
        assert_eq!(error.code, "E-MACRO-007");
    }

    #[test]
    fn hygiene_respects_lexical_branch_scope() {
        let source = "
(macro scoped () expr-syntax
  (do (quote expr-syntax
    (do
      (if true (do (let branch-value 1) branch-value) (do unit))
      branch-value))))
(fn main () int64 (do (scoped)))";
        let expanded = expand_typed_macros(module(source, 9)).unwrap();
        let TopLevel::Function(function) = &expanded.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Do(values) = &function.body[0].value else {
            panic!("expected expanded do");
        };
        let ExprKind::If { then_body, .. } = &values[0].value else {
            panic!("expected if");
        };
        assert!(matches!(
            then_body[1].value,
            ExprKind::Reference(ref name) if name == "branch-value--macro-1"
        ));
        assert!(matches!(
            values[1].value,
            ExprKind::Reference(ref name) if name == "branch-value"
        ));
    }

    #[test]
    fn unsupported_definition_and_module_categories_are_rejected_honestly() {
        for category in ["definition-syntax", "module-syntax"] {
            let source = format!(
                "(macro unsupported () {category} (do (quote {category} (const x int64 1))))"
            );
            let error = expand_typed_macros(module(&source, 4)).unwrap_err();
            assert_eq!(error.code, "E-MACRO-011");
        }
    }

    #[test]
    fn expansion_depth_limit_retains_call_and_definition_locations() {
        let source = "
(macro recurse ((value expr-syntax)) expr-syntax
  (do (quote expr-syntax (recurse (unquote value)))))
(fn main () int64 (do (recurse 1)))";
        let error = expand_typed_macros(module(source, 22)).unwrap_err();
        assert_eq!(error.code, "E-MACRO-006");
        assert_eq!(error.document, Some(DocumentId::from_raw(22)));
        assert_eq!(error.related[0].1.document, DocumentId::from_raw(22));
    }

    #[test]
    fn hard_limits_have_stable_diagnostic_codes() {
        let origin = Origin::DocumentSource {
            document: DocumentId::from_raw(3),
            ast_id: super::super::AstId::from_raw(4),
            span: Span::new(5, 6),
        };
        let mut state = ExpansionState {
            evaluation_steps: TYPED_MACRO_MAX_EVALUATION_STEPS,
            ..ExpansionState::default()
        };
        assert_eq!(step(&mut state, &origin).unwrap_err().code, "E-MACRO-008");
        let mut state = ExpansionState {
            generated_nodes: TYPED_MACRO_MAX_GENERATED_NODES,
            ..ExpansionState::default()
        };
        assert_eq!(
            generated(&mut state, &origin).unwrap_err().code,
            "E-MACRO-009"
        );
    }
}
