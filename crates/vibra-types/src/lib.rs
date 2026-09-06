//! Primitive type checking and lowering for the M2 Step 6 profile.
//!
//! The checker consumes the syntax AST and returns an immutable checked IR.
//! It admits primitive module values, direct local bindings, conditionals,
//! sequences, and fixed positional calls. A parsed AST that contains a
//! later-step form never crosses into `vibra-ir` or `vibra-interp`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CheckedFunction, CheckedGlobal, CheckedProgram, Expr, FunctionSignature,
    PrimitiveType, SourceOrigin, Value,
};
use vibra_syntax::{
    Attribute, Declaration, Expression, ExpressionKind, FloatSuffix, IntegerSuffix,
    Literal, NameKind, PatternKind, SourceAst, TypeExpr,
};

/// The result of checking one source document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    program: Option<CheckedProgram>,
    diagnostics: Vec<Diagnostic>,
}

impl CheckResult {
    /// Creates a semantic result.  Callers normally use [`check_source`].
    #[must_use]
    pub fn new(program: Option<CheckedProgram>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            program,
            diagnostics,
        }
    }

    /// The executable checked program, when the source contains a function.
    ///
    /// A constant-only module can be accepted with no executable entry point,
    /// so callers must use [`Self::accepted`] separately from this accessor.
    #[must_use]
    pub const fn program(&self) -> Option<&CheckedProgram> {
        self.program.as_ref()
    }

    /// Diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether checking accepted this source document.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.level() != vibra_diagnostics::Level::Error)
    }
}

/// Checks one `.vib` source document through the shared reader and lowers the
/// Step 6 subset to controlled IR.
pub fn check_source(source_id: impl AsRef<str>, source: &str) -> CheckResult {
    let source_id = source_id.as_ref();
    let document = match vibra_syntax::parse_source(Path::new(source_id), source) {
        Ok(document) => document,
        Err(error) => {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::ModuleIoError,
                ByteSpan::empty_at(0),
                error.to_string(),
            )
            .with_source_id(source_id);
            return CheckResult::new(None, vec![diagnostic]);
        }
    };

    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(source_id))
        .collect::<Vec<_>>();
    if !document.accepted() || document.recovered() {
        return CheckResult::new(None, diagnostics);
    }
    let Some(ast) = document.ast() else {
        if source.trim().is_empty() {
            unavailable(
                &mut diagnostics,
                source_id,
                ByteSpan::empty_at(0),
                "source contains no Step 6 module value or executable function",
            );
        }
        return CheckResult::new(None, diagnostics);
    };
    let program = check_ast(source_id, ast, &mut diagnostics);
    if !diagnostics
        .iter()
        .all(|diagnostic| diagnostic.level() != vibra_diagnostics::Level::Error)
    {
        return CheckResult::new(None, diagnostics);
    }
    CheckResult::new(program, diagnostics)
}

/// Checks an already decoded source AST.  This is useful to workspace
/// adapters that have already performed the reader phase.
pub fn check_ast(
    source_id: impl AsRef<str>,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CheckedProgram> {
    let source_id = source_id.as_ref();
    let mut checker = Checker::new(source_id, diagnostics, ast);
    checker.collect_headers();
    checker.check_initializer_cycles();
    checker.check_function_cycles();
    checker.check_globals();
    checker.check_functions();
    checker.finish()
}

#[derive(Clone)]
struct GlobalHeader {
    name: String,
    value_type: PrimitiveType,
    expression: Expression,
    span: ByteSpan,
}

#[derive(Clone)]
struct FunctionHeader {
    declaration_index: usize,
    name: String,
    signature: FunctionSignature,
}

#[derive(Clone)]
struct LocalBinding {
    slot: usize,
    value_type: PrimitiveType,
    span: ByteSpan,
}

struct Checker<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    ast: &'a SourceAst,
    globals: Vec<GlobalHeader>,
    global_indices: BTreeMap<String, usize>,
    functions: Vec<FunctionHeader>,
    function_indices: BTreeMap<String, usize>,
    module_names: BTreeMap<String, ByteSpan>,
    checked_globals: Vec<Option<CheckedGlobal>>,
    checked_functions: Vec<Option<CheckedFunction>>,
}

impl<'a> Checker<'a> {
    fn new(
        source_id: &'a str,
        diagnostics: &'a mut Vec<Diagnostic>,
        ast: &'a SourceAst,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            ast,
            globals: Vec::new(),
            global_indices: BTreeMap::new(),
            functions: Vec::new(),
            function_indices: BTreeMap::new(),
            module_names: BTreeMap::new(),
            checked_globals: Vec::new(),
            checked_functions: Vec::new(),
        }
    }

    fn collect_headers(&mut self) {
        for (declaration_index, declaration) in
            self.ast.declarations().iter().enumerate()
        {
            match declaration {
                Declaration::Def(definition) => {
                    let Some(value_type) = primitive_type(definition.value_type())
                    else {
                        unavailable(
                            self.diagnostics,
                            self.source_id,
                            definition.span(),
                            "only primitive module values are available in Step 6",
                        );
                        continue;
                    };
                    let name = definition.name().value().to_owned();
                    if let Some(earlier) = self.module_names.get(&name).copied() {
                        redeclaration(
                            self.diagnostics,
                            self.source_id,
                            definition.name().value(),
                            definition.name().value(),
                            definition.span(),
                            earlier,
                        );
                        continue;
                    }
                    self.module_names.insert(name.clone(), definition.span());
                    let index = self.globals.len();
                    self.global_indices.insert(name.clone(), index);
                    self.globals.push(GlobalHeader {
                        name,
                        value_type,
                        expression: definition.expression().clone(),
                        span: definition.span(),
                    });
                }
                Declaration::Defn(function) => {
                    let Some(signature) =
                        check_signature(self.source_id, function, self.diagnostics)
                    else {
                        continue;
                    };
                    let name = function.name().value().to_owned();
                    if let Some(earlier) = self.module_names.get(&name).copied() {
                        redeclaration(
                            self.diagnostics,
                            self.source_id,
                            &name,
                            &name,
                            function.span(),
                            earlier,
                        );
                        continue;
                    }
                    self.module_names.insert(name.clone(), function.span());
                    let index = self.functions.len();
                    self.function_indices.insert(name.clone(), index);
                    self.functions.push(FunctionHeader {
                        declaration_index,
                        name,
                        signature,
                    });
                }
                Declaration::Import(_) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    declaration.span(),
                    "imports require the resolved multi-module checker",
                ),
                _ => unavailable(
                    self.diagnostics,
                    self.source_id,
                    declaration.span(),
                    "this declaration family is outside the Step 6 primitive profile",
                ),
            }
        }
        self.checked_globals = vec![None; self.globals.len()];
        self.checked_functions = vec![None; self.functions.len()];
    }

    fn check_initializer_cycles(&mut self) {
        let mut states = vec![VisitState::Unvisited; self.globals.len()];
        for index in 0..self.globals.len() {
            self.visit_global(index, &mut states);
        }
    }

    fn check_function_cycles(&mut self) {
        let mut dependencies = vec![BTreeSet::new(); self.functions.len()];
        for (index, header) in self.functions.iter().enumerate() {
            let Some(Declaration::Defn(function)) =
                self.ast.declarations().get(header.declaration_index)
            else {
                continue;
            };
            let Some(function_dependencies) = dependencies.get_mut(index) else {
                continue;
            };
            for expression in function.expressions() {
                collect_function_dependencies(
                    expression,
                    &self.function_indices,
                    function_dependencies,
                );
            }
            function_dependencies.remove(&index);
        }
        let mut states = vec![VisitState::Unvisited; self.functions.len()];
        for index in 0..self.functions.len() {
            self.visit_function(index, &dependencies, &mut states);
        }
    }

    fn visit_function(
        &mut self,
        index: usize,
        dependencies: &[BTreeSet<usize>],
        states: &mut [VisitState],
    ) {
        match states.get(index).copied() {
            Some(VisitState::Done) => return,
            Some(VisitState::Visiting) => {
                let Some(header) = self.functions.get(index) else {
                    return;
                };
                let Some(Declaration::Defn(function)) =
                    self.ast.declarations().get(header.declaration_index)
                else {
                    return;
                };
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    function.span(),
                    "recursive call groups remain unavailable until the tail-call step",
                );
                return;
            }
            Some(VisitState::Unvisited) => {}
            None => return,
        }
        let Some(state) = states.get_mut(index) else {
            return;
        };
        *state = VisitState::Visiting;
        if let Some(function_dependencies) = dependencies.get(index) {
            for dependency in function_dependencies {
                self.visit_function(*dependency, dependencies, states);
            }
        }
        if let Some(state) = states.get_mut(index) {
            *state = VisitState::Done;
        }
    }

    fn visit_global(&mut self, index: usize, states: &mut [VisitState]) {
        match states.get(index).copied() {
            Some(VisitState::Done) => return,
            Some(VisitState::Visiting) => {
                let Some(header) = self.globals.get(index) else {
                    return;
                };
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeInitializerCycle,
                        header.span,
                        "module value initializers form a cycle",
                    )
                    .with_source_id(self.source_id),
                );
                return;
            }
            Some(VisitState::Unvisited) => {}
            None => return,
        }
        let Some(state) = states.get_mut(index) else {
            return;
        };
        *state = VisitState::Visiting;
        let mut dependencies = BTreeSet::new();
        let Some(expression) = self.globals.get(index).map(|header| &header.expression)
        else {
            return;
        };
        collect_global_dependencies(
            expression,
            &self.global_indices,
            &self.function_indices,
            &self.functions,
            self.ast,
            &mut dependencies,
        );
        for dependency in dependencies {
            self.visit_global(dependency, states);
        }
        if let Some(state) = states.get_mut(index) {
            *state = VisitState::Done;
        }
    }

    fn check_globals(&mut self) {
        for index in 0..self.globals.len() {
            let Some(header) = self.globals.get(index).cloned() else {
                continue;
            };
            let mut environment = CheckEnvironment::new(
                self.source_id,
                self.diagnostics,
                &self.global_indices,
                &self.globals,
                &self.functions,
                &self.function_indices,
                &self.module_names,
                None,
            );
            let Some(expression) = check_expression(
                &mut environment,
                &header.expression,
                Some(header.value_type),
            ) else {
                continue;
            };
            let origin = SourceOrigin::new(self.source_id, header.span);
            match CheckedGlobal::new(header.name, header.value_type, expression, origin)
            {
                Ok(global) => {
                    if let Some(slot) = self.checked_globals.get_mut(index) {
                        *slot = Some(global);
                    }
                }
                Err(error) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    header.span,
                    format!("checked IR construction failed: {error}"),
                ),
            }
        }
    }

    fn check_functions(&mut self) {
        for (index, header) in self.functions.clone().into_iter().enumerate() {
            let Some(Declaration::Defn(function)) =
                self.ast.declarations().get(header.declaration_index)
            else {
                continue;
            };
            if has_deferred_attributes(function.attributes().items()) {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    function.span(),
                    "labelled, variadic, generic, external, and nonempty-effect attributes are unavailable in Step 6",
                );
                continue;
            }
            let mut environment = CheckEnvironment::new(
                self.source_id,
                self.diagnostics,
                &self.global_indices,
                &self.globals,
                &self.functions,
                &self.function_indices,
                &self.module_names,
                Some(index),
            );
            let mut parameters_valid = true;
            for (parameter_index, parameter) in function.parameters().iter().enumerate()
            {
                match parameter.parsed_pattern().kind() {
                    PatternKind::Binding(name) if name.is_discard() => {}
                    PatternKind::Binding(name) => {
                        if !environment.add_binding(
                            name.value(),
                            parameter.value_type(),
                            parameter.span(),
                        ) {
                            parameters_valid = false;
                        }
                    }
                    _ => {
                        parameters_valid = false;
                        unavailable(
                            environment.diagnostics,
                            environment.source_id,
                            parameter.span(),
                            "constructor and destructuring patterns are deferred until M3",
                        );
                    }
                }
                environment.next_slot = parameter_index.saturating_add(1);
            }
            if !parameters_valid {
                continue;
            }
            let Some(body) = check_sequence(
                &mut environment,
                function.expressions(),
                Some(header.signature.result()),
                function.span(),
            ) else {
                continue;
            };
            let origin = SourceOrigin::new(self.source_id, function.span());
            match CheckedFunction::with_slots(
                header.name,
                header.signature,
                body,
                origin,
                environment.next_slot,
            ) {
                Ok(function) => {
                    if let Some(slot) = self.checked_functions.get_mut(index) {
                        *slot = Some(function);
                    }
                }
                Err(error) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    function.span(),
                    format!("checked IR construction failed: {error}"),
                ),
            }
        }
    }

    fn finish(self) -> Option<CheckedProgram> {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error)
        {
            return None;
        }
        let globals = self
            .checked_globals
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let functions = self
            .checked_functions
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        if functions.is_empty() {
            if globals.is_empty() {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    self.ast.span(),
                    "source contains no Step 6 module value or executable function",
                );
            }
            return None;
        }
        match CheckedProgram::try_new_with_globals(globals, functions, 0) {
            Ok(program) => Some(program),
            Err(error) => {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    self.ast.span(),
                    format!("checked IR construction failed: {error}"),
                );
                None
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Done,
}

struct CheckEnvironment<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    global_indices: &'a BTreeMap<String, usize>,
    globals: &'a [GlobalHeader],
    functions: &'a [FunctionHeader],
    function_indices: &'a BTreeMap<String, usize>,
    module_names: &'a BTreeMap<String, ByteSpan>,
    locals: BTreeMap<String, LocalBinding>,
    next_slot: usize,
    current_function: Option<usize>,
}

impl<'a> CheckEnvironment<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        source_id: &'a str,
        diagnostics: &'a mut Vec<Diagnostic>,
        global_indices: &'a BTreeMap<String, usize>,
        globals: &'a [GlobalHeader],
        functions: &'a [FunctionHeader],
        function_indices: &'a BTreeMap<String, usize>,
        module_names: &'a BTreeMap<String, ByteSpan>,
        current_function: Option<usize>,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            global_indices,
            globals,
            functions,
            function_indices,
            module_names,
            locals: BTreeMap::new(),
            next_slot: 0,
            current_function,
        }
    }

    fn add_binding(
        &mut self,
        name: &str,
        value_type: &TypeExpr,
        span: ByteSpan,
    ) -> bool {
        let Some(value_type) = primitive_type(value_type) else {
            unavailable(
                self.diagnostics,
                self.source_id,
                span,
                "only primitive binding types are available in Step 6",
            );
            return false;
        };
        self.add_binding_type(name, value_type, span)
    }

    fn add_binding_type(
        &mut self,
        name: &str,
        value_type: PrimitiveType,
        span: ByteSpan,
    ) -> bool {
        if let Some(earlier) = self.module_names.get(name).copied() {
            redeclaration(self.diagnostics, self.source_id, name, name, span, earlier);
            return false;
        }
        if let Some(earlier) = self.locals.get(name) {
            redeclaration(
                self.diagnostics,
                self.source_id,
                name,
                name,
                span,
                earlier.span,
            );
            return false;
        }
        let slot = self.next_slot;
        self.locals.insert(
            name.to_owned(),
            LocalBinding {
                slot,
                value_type,
                span,
            },
        );
        self.next_slot = self.next_slot.saturating_add(1);
        true
    }
}

fn collect_global_dependencies(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
) {
    let mut visited_functions = BTreeSet::new();
    collect_global_dependencies_inner(
        expression,
        global_indices,
        function_indices,
        functions,
        ast,
        dependencies,
        &mut visited_functions,
    );
}

fn collect_global_dependencies_inner(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
    visited_functions: &mut BTreeSet<usize>,
) {
    match expression.kind() {
        ExpressionKind::Name(name)
            if name.kind() == NameKind::Symbol && name.segments().len() == 1 =>
        {
            if let Some(index) = global_indices.get(name.value()).copied() {
                dependencies.insert(index);
            }
        }
        ExpressionKind::Application(application) => {
            if let ExpressionKind::Name(name) = application.callee().kind()
                && name.kind() == NameKind::Symbol
                && name.segments().len() == 1
                && let Some(function_index) =
                    function_indices.get(name.value()).copied()
                && visited_functions.insert(function_index)
                && let Some(header) = functions.get(function_index)
                && let Some(Declaration::Defn(function)) =
                    ast.declarations().get(header.declaration_index)
            {
                for expression in function.expressions() {
                    collect_global_dependencies_inner(
                        expression,
                        global_indices,
                        function_indices,
                        functions,
                        ast,
                        dependencies,
                        visited_functions,
                    );
                }
            }
            collect_global_dependencies_inner(
                application.callee(),
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for argument in application.arguments() {
                collect_global_dependencies_inner(
                    argument.value(),
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Let { value, body, .. } => {
            collect_global_dependencies_inner(
                value,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for expression in body {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_global_dependencies_inner(
                condition,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            collect_global_dependencies_inner(
                then_branch,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            collect_global_dependencies_inner(
                else_branch,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_global_dependencies_inner(
                scrutinee,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for arm in arms {
                collect_global_dependencies_inner(
                    arm.result(),
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_global_dependencies_inner(
                operand,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn collect_function_dependencies(
    expression: &Expression,
    function_indices: &BTreeMap<String, usize>,
    dependencies: &mut BTreeSet<usize>,
) {
    match expression.kind() {
        ExpressionKind::Application(application) => {
            if let ExpressionKind::Name(name) = application.callee().kind()
                && name.kind() == NameKind::Symbol
                && name.segments().len() == 1
                && let Some(index) = function_indices.get(name.value()).copied()
            {
                dependencies.insert(index);
            }
            collect_function_dependencies(
                application.callee(),
                function_indices,
                dependencies,
            );
            for argument in application.arguments() {
                collect_function_dependencies(
                    argument.value(),
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Let { value, body, .. } => {
            collect_function_dependencies(value, function_indices, dependencies);
            for expression in body {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_function_dependencies(condition, function_indices, dependencies);
            collect_function_dependencies(then_branch, function_indices, dependencies);
            collect_function_dependencies(else_branch, function_indices, dependencies);
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_function_dependencies(scrutinee, function_indices, dependencies);
            for arm in arms {
                collect_function_dependencies(
                    arm.result(),
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_function_dependencies(operand, function_indices, dependencies);
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn ensure_expected(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    expected: Option<PrimitiveType>,
    actual: PrimitiveType,
) {
    if let Some(expected) = expected
        && expected != actual
    {
        mismatch(
            environment.diagnostics,
            environment.source_id,
            span,
            expected,
            actual,
            "expression type does not match the written expectation",
        );
    }
}

fn unknown_name(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    name: &str,
) {
    environment.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::NameUnknownSymbol,
            expression.span(),
            format!("symbol `{name}` does not resolve to a visible value"),
        )
        .with_source_id(environment.source_id),
    );
}

fn redeclaration(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    _name: &str,
    _earlier_name: &str,
    span: ByteSpan,
    earlier_span: ByteSpan,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::NameRedeclaration,
            span,
            "a visible name is introduced more than once",
        )
        .with_source_id(source_id)
        .with_related_source(
            source_id,
            earlier_span,
            "the earlier binding is here",
        ),
    );
}

fn check_signature(
    source_id: &str,
    function: &vibra_syntax::FunctionDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut parameters = Vec::with_capacity(function.parameters().len());
    let mut valid = true;
    for parameter in function.parameters() {
        match primitive_type(parameter.value_type()) {
            Some(value_type) => parameters.push(value_type),
            None => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    parameter.span(),
                    "only primitive parameter types are available in Step 5",
                );
            }
        }
    }
    let result = match primitive_type(function.result()) {
        Some(value_type) => value_type,
        None => {
            valid = false;
            unavailable(
                diagnostics,
                source_id,
                function.span(),
                "only primitive result types are available in Step 5",
            );
            PrimitiveType::Void
        }
    };
    valid.then(|| FunctionSignature::new(parameters, result))
}

fn primitive_type(value: &TypeExpr) -> Option<PrimitiveType> {
    match value {
        TypeExpr::Void => Some(PrimitiveType::Void),
        TypeExpr::Name(name) => match name.value() {
            "bool" => Some(PrimitiveType::Bool),
            "char" => Some(PrimitiveType::Char),
            "str" => Some(PrimitiveType::Str),
            "bytes" => Some(PrimitiveType::Bytes),
            "atom" => Some(PrimitiveType::Atom),
            "i8" => Some(PrimitiveType::I8),
            "i16" => Some(PrimitiveType::I16),
            "i32" => Some(PrimitiveType::I32),
            "i64" => Some(PrimitiveType::I64),
            "u8" => Some(PrimitiveType::U8),
            "u16" => Some(PrimitiveType::U16),
            "u32" => Some(PrimitiveType::U32),
            "u64" => Some(PrimitiveType::U64),
            "f32" => Some(PrimitiveType::F32),
            "f64" => Some(PrimitiveType::F64),
            _ => None,
        },
        TypeExpr::Applied { .. }
        | TypeExpr::Tuple(_)
        | TypeExpr::Array(_)
        | TypeExpr::Map(_, _)
        | TypeExpr::Function(_) => None,
    }
}

fn has_deferred_attributes(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| match attribute {
        Attribute::Where(_)
        | Attribute::Labelled(_)
        | Attribute::Variadic(_)
        | Attribute::External(_)
        | Attribute::Symbol(_) => true,
        Attribute::Effects(row) => !row.references().is_empty(),
        Attribute::Visibility(_) | Attribute::Doc(_) => false,
    })
}

fn check_sequence(
    environment: &mut CheckEnvironment<'_>,
    expressions: &[Expression],
    expected: Option<PrimitiveType>,
    fallback_span: ByteSpan,
) -> Option<Expr> {
    if expressions.is_empty() {
        if expected.is_some_and(|expected| expected != PrimitiveType::Void) {
            mismatch(
                environment.diagnostics,
                environment.source_id,
                fallback_span,
                expected.unwrap_or(PrimitiveType::Void),
                PrimitiveType::Void,
                "an empty sequence returns void",
            );
            return None;
        }
        return Some(Expr::literal(
            Value::Void,
            SourceOrigin::new(environment.source_id, fallback_span),
        ));
    }

    let mut checked = Vec::with_capacity(expressions.len());
    let mut valid = true;
    for (index, expression) in expressions.iter().enumerate() {
        let expression_expected = (index + 1 == expressions.len())
            .then_some(expected)
            .flatten();
        match check_expression(environment, expression, expression_expected) {
            Some(value) => checked.push(value),
            None => valid = false,
        }
    }
    if !valid {
        return None;
    }
    let origin = expressions
        .first()
        .zip(expressions.last())
        .map(|(first, last)| {
            SourceOrigin::new(environment.source_id, first.span().join(last.span()))
        })
        .unwrap_or_else(|| SourceOrigin::new(environment.source_id, fallback_span));
    Some(Expr::sequence(checked, origin))
}

fn check_expression(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<PrimitiveType>,
) -> Option<Expr> {
    match expression.kind() {
        ExpressionKind::Literal(literal) => check_literal(
            environment.source_id,
            expression.span(),
            literal,
            expected,
            environment.diagnostics,
        )
        .map(|value| {
            Expr::literal(
                value,
                SourceOrigin::new(environment.source_id, expression.span()),
            )
        }),
        ExpressionKind::Name(name) if name.kind() == NameKind::Atom => {
            let actual = PrimitiveType::Atom;
            if expected.is_some_and(|expected| expected != actual) {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    expression.span(),
                    expected.unwrap_or(actual),
                    actual,
                    "an atom literal does not match the written result type",
                );
                return None;
            }
            Some(Expr::literal(
                Value::Atom(name.value().to_owned()),
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ExpressionKind::Name(name) if name.kind() == NameKind::Symbol => {
            if name.segments().len() != 1 {
                unknown_name(environment, expression, name.value());
                return None;
            }
            if let Some(binding) = environment.locals.get(name.value()).cloned() {
                ensure_expected(
                    environment,
                    expression.span(),
                    expected,
                    binding.value_type,
                );
                return (expected.is_none() || expected == Some(binding.value_type))
                    .then_some(Expr::variable(
                        binding.slot,
                        binding.value_type,
                        SourceOrigin::new(environment.source_id, expression.span()),
                    ));
            }
            if let Some(index) = environment.global_indices.get(name.value()).copied() {
                let global = environment.globals.get(index)?;
                let actual = global.value_type;
                ensure_expected(environment, expression.span(), expected, actual);
                return (expected.is_none() || expected == Some(actual)).then_some(
                    Expr::global(
                        index,
                        actual,
                        SourceOrigin::new(environment.source_id, expression.span()),
                    ),
                );
            }
            if environment.function_indices.contains_key(name.value()) {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    expression.span(),
                    "function values are deferred until the callable-value step",
                );
            } else {
                unknown_name(environment, expression, name.value());
            }
            None
        }
        ExpressionKind::Application(application) => {
            if application.type_arguments().is_some() {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    application.span(),
                    "generic type arguments are deferred until M3",
                );
                return None;
            }
            if application
                .arguments()
                .iter()
                .any(|argument| argument.label().is_some())
            {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    application.span(),
                    "labelled and variadic calls are deferred until Step 7",
                );
                return None;
            }
            let ExpressionKind::Name(name) = application.callee().kind() else {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    application.callee().span(),
                    "only direct fixed positional calls are available in Step 6",
                );
                return None;
            };
            if name.kind() != NameKind::Symbol || name.segments().len() != 1 {
                unknown_name(environment, application.callee(), name.value());
                return None;
            }
            let Some(function_index) =
                environment.function_indices.get(name.value()).copied()
            else {
                unknown_name(environment, application.callee(), name.value());
                return None;
            };
            if environment.current_function == Some(function_index) {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    application.span(),
                    "recursive calls remain unavailable until the tail-call step",
                );
                return None;
            }
            let header = environment.functions.get(function_index)?;
            if header.signature.parameters().len() != application.arguments().len() {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    application.span(),
                    PrimitiveType::Void,
                    PrimitiveType::Void,
                    format!(
                        "call to `{}` has {} arguments, expected {}",
                        name.value(),
                        application.arguments().len(),
                        header.signature.parameters().len()
                    ),
                );
                return None;
            }
            let mut arguments = Vec::with_capacity(application.arguments().len());
            for (argument, value_type) in application
                .arguments()
                .iter()
                .zip(header.signature.parameters())
            {
                let value =
                    check_expression(environment, argument.value(), Some(*value_type))?;
                arguments.push(value);
            }
            ensure_expected(
                environment,
                application.span(),
                expected,
                header.signature.result(),
            );
            (expected.is_none() || expected == Some(header.signature.result()))
                .then_some(Expr::call(
                    function_index,
                    arguments,
                    header.signature.result(),
                    SourceOrigin::new(environment.source_id, application.span()),
                ))
        }
        ExpressionKind::Do(expressions) => {
            check_sequence(environment, expressions, expected, expression.span())
        }
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            let value = check_expression(environment, value, None)?;
            let mut nested = CheckEnvironment {
                source_id: environment.source_id,
                diagnostics: environment.diagnostics,
                global_indices: environment.global_indices,
                globals: environment.globals,
                functions: environment.functions,
                function_indices: environment.function_indices,
                module_names: environment.module_names,
                locals: environment.locals.clone(),
                next_slot: environment.next_slot,
                current_function: environment.current_function,
            };
            let slot = match pattern.kind() {
                PatternKind::Binding(name) if name.is_discard() => None,
                PatternKind::Binding(name) => {
                    if !nested.add_binding_type(
                        name.value(),
                        value.result_type(),
                        pattern.span(),
                    ) {
                        return None;
                    }
                    Some(nested.next_slot.saturating_sub(1))
                }
                _ => {
                    unavailable(
                        nested.diagnostics,
                        nested.source_id,
                        pattern.span(),
                        "constructor and destructuring patterns are deferred until M3",
                    );
                    return None;
                }
            };
            let result = check_sequence(&mut nested, body, expected, expression.span());
            environment.next_slot = environment.next_slot.max(nested.next_slot);
            result.map(|body| {
                Expr::let_binding(
                    slot,
                    value,
                    body,
                    SourceOrigin::new(environment.source_id, expression.span()),
                )
            })
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition =
                check_expression(environment, condition, Some(PrimitiveType::Bool))?;
            let then_branch = check_expression(environment, then_branch, expected)?;
            let else_branch = check_expression(environment, else_branch, expected)?;
            if then_branch.result_type() != else_branch.result_type() {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    expression.span(),
                    then_branch.result_type(),
                    else_branch.result_type(),
                    "if branches must have identical types",
                );
                return None;
            }
            Some(Expr::if_expression(
                condition,
                then_branch,
                else_branch,
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ExpressionKind::Lambda(_)
        | ExpressionKind::Match { .. }
        | ExpressionKind::As { .. }
        | ExpressionKind::Try(_) => {
            unavailable(
                environment.diagnostics,
                environment.source_id,
                expression.span(),
                "this expression form is deferred until a later M2/M3 step",
            );
            None
        }
        ExpressionKind::Name(_) => {
            unknown_name(environment, expression, "<invalid name>");
            None
        }
    }
}

fn check_literal(
    source_id: &str,
    span: ByteSpan,
    literal: &Literal,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    match literal {
        Literal::String(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Str,
            Value::Str(value.value().to_owned()),
            diagnostics,
        ),
        Literal::Character(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Char,
            Value::Char(value.value()),
            diagnostics,
        ),
        Literal::Boolean(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Bool,
            Value::Bool(value.value()),
            diagnostics,
        ),
        Literal::Void(_) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Void,
            Value::Void,
            diagnostics,
        ),
        Literal::Integer(value) => {
            check_integer(source_id, span, value, expected, diagnostics)
        }
        Literal::Float(value) => {
            check_float(source_id, span, value, expected, diagnostics)
        }
    }
}

fn expect_fixed(
    source_id: &str,
    span: ByteSpan,
    expected: Option<PrimitiveType>,
    actual: PrimitiveType,
    value: Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    if expected.is_some_and(|expected| expected != actual) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(actual),
            actual,
            "literal type does not match the written result type",
        );
        None
    } else {
        Some(value)
    }
}

fn check_integer(
    source_id: &str,
    span: ByteSpan,
    literal: &vibra_syntax::IntegerLiteral,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(integer_suffix_type)
        .or_else(|| expected.filter(|value| value.is_integer()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::I64),
            PrimitiveType::I64,
            "an unsuffixed integer needs one expected integer type",
        );
        return None;
    };
    let Some(magnitude) = literal.digits().parse::<u128>().ok() else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "integer digits exceed all v1 widths",
        );
        return None;
    };
    let Some(value) = integer_value(target, literal.is_negative(), magnitude) else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "integer is outside its exact primitive range",
        );
        return None;
    };
    if expected.is_some_and(|expected| expected != target) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(target),
            target,
            "numeric suffixes never request an implicit conversion",
        );
        return None;
    }
    Some(value)
}

fn check_float(
    source_id: &str,
    span: ByteSpan,
    literal: &vibra_syntax::FloatLiteral,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(float_suffix_type)
        .or_else(|| expected.filter(|value| value.is_float()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::F64),
            PrimitiveType::F64,
            "an unsuffixed float needs one expected floating-point type",
        );
        return None;
    };
    let value = match target {
        PrimitiveType::F32 => literal
            .body()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f32),
        PrimitiveType::F64 => literal
            .body()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f64),
        _ => None,
    };
    let Some(value) = value else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "finite float literal overflows its exact type",
        );
        return None;
    };
    if expected.is_some_and(|expected| expected != target) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(target),
            target,
            "numeric suffixes never request an implicit conversion",
        );
        return None;
    }
    Some(value)
}

fn integer_suffix_type(suffix: IntegerSuffix) -> PrimitiveType {
    match suffix {
        IntegerSuffix::I8 => PrimitiveType::I8,
        IntegerSuffix::I16 => PrimitiveType::I16,
        IntegerSuffix::I32 => PrimitiveType::I32,
        IntegerSuffix::I64 => PrimitiveType::I64,
        IntegerSuffix::U8 => PrimitiveType::U8,
        IntegerSuffix::U16 => PrimitiveType::U16,
        IntegerSuffix::U32 => PrimitiveType::U32,
        IntegerSuffix::U64 => PrimitiveType::U64,
    }
}

fn float_suffix_type(suffix: FloatSuffix) -> PrimitiveType {
    match suffix {
        FloatSuffix::F32 => PrimitiveType::F32,
        FloatSuffix::F64 => PrimitiveType::F64,
    }
}

fn integer_value(
    target: PrimitiveType,
    negative: bool,
    magnitude: u128,
) -> Option<Value> {
    match target {
        PrimitiveType::I8 => {
            signed_value(negative, magnitude, i8::MIN as i128, i8::MAX as i128)
                .map(|value| Value::I8(value as i8))
        }
        PrimitiveType::I16 => {
            signed_value(negative, magnitude, i16::MIN as i128, i16::MAX as i128)
                .map(|value| Value::I16(value as i16))
        }
        PrimitiveType::I32 => {
            signed_value(negative, magnitude, i32::MIN as i128, i32::MAX as i128)
                .map(|value| Value::I32(value as i32))
        }
        PrimitiveType::I64 => {
            signed_value(negative, magnitude, i64::MIN as i128, i64::MAX as i128)
                .map(|value| Value::I64(value as i64))
        }
        PrimitiveType::U8 => (!negative && magnitude <= u8::MAX as u128)
            .then_some(Value::U8(magnitude as u8)),
        PrimitiveType::U16 => (!negative && magnitude <= u16::MAX as u128)
            .then_some(Value::U16(magnitude as u16)),
        PrimitiveType::U32 => (!negative && magnitude <= u32::MAX as u128)
            .then_some(Value::U32(magnitude as u32)),
        PrimitiveType::U64 => (!negative && magnitude <= u64::MAX as u128)
            .then_some(Value::U64(magnitude as u64)),
        _ => None,
    }
}

fn signed_value(negative: bool, magnitude: u128, min: i128, max: i128) -> Option<i128> {
    if negative {
        let magnitude = i128::try_from(magnitude).ok()?;
        let value = magnitude.checked_neg()?;
        (value >= min).then_some(value)
    } else {
        let value = i128::try_from(magnitude).ok()?;
        (value <= max).then_some(value)
    }
}

fn mismatch(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    expected: PrimitiveType,
    actual: PrimitiveType,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeArgumentMismatch,
            span,
            format!("{}: expected {expected}, found {actual}", message.into()),
        )
        .with_source_id(source_id),
    );
}

fn out_of_range(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    message: &'static str,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::TypeNumericOutOfRange, span, message)
            .with_source_id(source_id),
    );
}

fn unavailable(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::ToolUnavailable, span, message)
            .with_source_id(source_id),
    );
}

#[cfg(test)]
mod tests {
    use super::check_source;
    use vibra_diagnostics::DiagnosticCode;

    #[test]
    fn checks_an_unsuffixed_integer_against_the_written_result() {
        let result = check_source("answer.vib", "(defn answer () i32 42)");
        assert!(result.accepted(), "{:?}", result.diagnostics());
        assert_eq!(
            result
                .program()
                .expect("program")
                .entry()
                .body()
                .result_type(),
            vibra_ir::PrimitiveType::I32
        );
    }

    #[test]
    fn rejects_an_integer_that_exceeds_its_suffix() {
        let result = check_source("answer.vib", "(defn answer () i8 128i8)");
        assert!(!result.accepted());
        assert!(result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == DiagnosticCode::TypeNumericOutOfRange
        ));
    }

    #[test]
    fn rejects_wrong_result_without_lowering() {
        let result = check_source("answer.vib", "(defn answer () str 1i32)");
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        ));
    }

    #[test]
    fn preserves_direct_f32_rounding_without_a_f64_intermediate() {
        let result = check_source(
            "rounding.vib",
            "(defn answer () f32 1.00000011920928955078125)",
        );
        let value = result
            .program()
            .expect("checked rounding program")
            .entry()
            .body()
            .expressions()
            .first()
            .expect("expression")
            .literal_value()
            .expect("literal")
            .as_f32()
            .expect("f32 value");
        assert_eq!(value.to_bits(), 1.0000001_f32.to_bits());
    }

    #[test]
    fn rejects_negative_unsigned_and_float_overflow_before_lowering() {
        for source in ["(defn answer () u8 -1)", "(defn answer () f32 1e+39f32)"] {
            let result = check_source("range.vib", source);
            assert!(!result.accepted(), "{source}");
            assert!(result.program().is_none());
            assert!(result.diagnostics().iter().any(|diagnostic| {
                diagnostic.code() == DiagnosticCode::TypeNumericOutOfRange
            }));
        }
    }

    #[test]
    fn typed_module_values_enter_the_checked_program_boundary() {
        let result =
            check_source("deferred.vib", "(def value i32 1)\n(defn answer () i32 2)");
        assert!(result.accepted(), "{:?}", result.diagnostics());
        assert_eq!(result.program().expect("program").globals().len(), 1);
    }

    #[test]
    fn checks_bindings_conditionals_and_fixed_calls() {
        let result = check_source(
            "bindings.vib",
            "(def base i32 40)\n(defn answer () i32 (add base))\n(defn add (value i32) i32 (let next (do 1i8 2i8) (if true (do value 2i32) 0i32)))",
        );
        assert!(result.accepted(), "{:?}", result.diagnostics());
        let program = result.program().expect("program");
        assert_eq!(program.functions().len(), 2);
        assert!(matches!(
            program.entry().body().expressions().first(),
            Some(vibra_ir::Expr::Call { .. })
        ));
    }

    #[test]
    fn rejects_initializer_cycles_before_execution() {
        let result = check_source(
            "cycle.vib",
            "(def first i32 second)\n(def second i32 first)\n(defn answer () i32 first)",
        );
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeInitializerCycle
        }));
    }

    #[test]
    fn rejects_shadowing_and_non_boolean_condition() {
        let result = check_source(
            "scope.vib",
            "(def value i32 1)\n(defn answer (value i32) i32 (let value value (if value 1i32 2i32)))",
        );
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::NameRedeclaration
        }));
        let condition = check_source(
            "condition.vib",
            "(defn answer (value i32) i32 (if value 1i32 2i32))",
        );
        assert!(condition.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        }));
    }

    #[test]
    fn local_shadowing_reports_binder_spans() {
        let source =
            "(defn answer (value i32) i32 (let first value (let first 1i32 first)))";
        let result = check_source("scope.vib", source);
        let diagnostic = result
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
            .expect("local redeclaration");
        assert_eq!(
            diagnostic.primary_span(),
            vibra_diagnostics::ByteSpan::new(51, 56)
        );
        assert_eq!(diagnostic.related().len(), 1);
        assert_eq!(
            diagnostic.related()[0].span,
            vibra_diagnostics::ByteSpan::new(34, 39)
        );
    }

    #[test]
    fn indirect_initializer_cycles_follow_fixed_calls() {
        let result = check_source(
            "indirect-cycle.vib",
            "(def first i32 (read))\n(def second i32 first)\n(defn read () i32 second)\n(defn answer () i32 first)",
        );
        assert!(!result.accepted());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeInitializerCycle
        }));
    }

    #[test]
    fn rejects_mutual_recursive_calls_before_lowering() {
        let result = check_source(
            "recursive.vib",
            "(defn first () i32 (second))\n(defn second () i32 (first))\n(defn answer () i32 (first))",
        );
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
        }));
    }

    #[test]
    fn imports_do_not_cross_the_source_only_checked_boundary() {
        let result =
            check_source("imports.vib", "(import io @std.io)\n(defn answer () i32 2)");
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
        }));
    }
}
