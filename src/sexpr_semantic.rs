//! S-expression-native semantic index over the typed surface AST.
//!
//! This is the editor-facing semantic index for S-expression source.
//!
//! This module builds the same four fact categories — definitions, imports,
//! references, and calls — directly from `crate::ast::{Module, TopLevel,
//! Expr, ...}` produced by [`crate::frontend::load_surface_program`]. Every
//! fact is located by a document-qualified half-open UTF-8 byte span
//! (`crate::ast::DocumentId` + `crate::syntax::Span`) instead of a mapping
//! path, so two documents that happen to share identical byte offsets never
//! collide (`DocumentId` is derived from the normalized document path; see
//! PR #161).
//!
//! Only symbols that can be looked up across module boundaries are indexed:
//! top-level definitions, import aliases, and value/type/pattern references
//! that name a definition, a call target, or a type. Purely lexical names
//! (`let`/`set` bindings, loop variables, task captures, record/field
//! labels) are not indexed.
//!
//! Source of truth is intentionally narrow: this module never parses YAML
//! or touches `serde_yaml`. It
//! only understands documents that already parse and type-check as
//! S-expression source through [`crate::frontend`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ast::{
    self, Annotation, AnnotationKind, DocumentId, Expr, ExprKind, ImplItem, Literal, MethodBinding,
    Pattern, PatternKind, TopLevel, TypeExpr, TypeExprKind,
};
use crate::frontend::{self, SurfaceProgram};
use crate::load::CompilationFlags;
use crate::syntax::Span;

/// The semantic fact categories for definitions, imports, references, calls,
/// and atom literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticKind {
    Definition,
    Import,
    Reference,
    Call,
    /// An `@name` atom literal. Atoms have no declaration site, so these
    /// facts drive hover, references, and completion but never definition.
    Atom,
}

/// One indexed fact, located by a document-qualified half-open byte span
/// rather than a YAML mapping path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFact {
    /// Physical source file this fact was read from.
    pub document: PathBuf,
    /// Stable identity of `document`, derived from its normalized path.
    /// Two facts with an identical `span` but different `document_id`
    /// never compare equal and are never conflated by callers that index
    /// facts by `(document_id, span)`.
    pub document_id: DocumentId,
    /// Half-open UTF-8 byte range `[start, end)` within `document`.
    pub span: Span,
    pub kind: SemanticKind,
    pub symbol: String,
    /// The module this fact resolves to, when known precisely:
    /// - `Import`: the resolved target module path.
    /// - `Definition`/top-level form: the physical document defining it
    ///   (always known exactly, since the fact's own `document` is that
    ///   document).
    /// - `Reference`/`Call`: the resolved target module path when `symbol`
    ///   is qualified through a known import alias declared in this
    ///   document's module; `None` for an unqualified, same-module
    ///   reference (resolve it by querying local `Definition` facts).
    pub target_document: Option<PathBuf>,
}

/// S-expression-native semantic index built from the typed surface AST.
#[derive(Debug, Clone, Default)]
pub struct SemanticIndex {
    facts: Vec<SemanticFact>,
    document_paths: BTreeMap<DocumentId, PathBuf>,
}

struct DocContext<'a> {
    document: &'a Path,
    document_id: DocumentId,
    import_targets: &'a BTreeMap<String, PathBuf>,
}

impl SemanticIndex {
    /// Parse, type-check, and index an entry module and its import graph.
    /// This is the only entry point that touches the filesystem; it is a
    /// thin wrapper over [`frontend::load_surface_program`] plus
    /// [`SemanticIndex::from_surface_program`].
    pub fn build(entry: &Path, flags: &CompilationFlags) -> Result<Self> {
        let program = frontend::load_surface_program(entry, flags)?;
        Ok(Self::from_surface_program(&program))
    }

    /// Index an already-loaded [`SurfaceProgram`]. Pure and infallible: the
    /// typed AST has already been parsed and validated by the frontend.
    pub fn from_surface_program(program: &SurfaceProgram) -> Self {
        let mut facts = Vec::new();
        let mut document_paths = BTreeMap::new();
        let empty_imports = BTreeMap::new();

        for (module_path, module) in &program.modules {
            let import_targets = program.imports.get(module_path).unwrap_or(&empty_imports);
            for part in &module.parts {
                let document_id = part.module.document_id;
                document_paths.insert(document_id, part.path.clone());
                let context = DocContext {
                    document: &part.path,
                    document_id,
                    import_targets,
                };
                for form in &part.module.forms {
                    index_top_level(form, &context, &mut facts);
                }
            }
        }
        Self {
            facts,
            document_paths,
        }
    }

    /// Filter facts by kind and/or exact symbol text.
    pub fn query(&self, kind: Option<SemanticKind>, symbol: Option<&str>) -> Vec<&SemanticFact> {
        self.facts
            .iter()
            .filter(|fact| kind.is_none_or(|kind| fact.kind == kind))
            .filter(|fact| symbol.is_none_or(|symbol| fact.symbol == symbol))
            .collect()
    }

    /// Every indexed fact, in build order.
    pub fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    /// Recover the physical path for a `DocumentId` seen in this index.
    pub fn document_path(&self, id: DocumentId) -> Option<&Path> {
        self.document_paths.get(&id).map(PathBuf::as_path)
    }
}

fn index_top_level(form: &TopLevel, context: &DocContext<'_>, facts: &mut Vec<SemanticFact>) {
    match form {
        TopLevel::Import(import) => {
            let symbol = import.alias.value.clone();
            let target = context.import_targets.get(&symbol).cloned();
            push(
                facts,
                context,
                import.alias.span,
                SemanticKind::Import,
                symbol,
                target,
            );
        }
        TopLevel::Definition(definition) => {
            push(
                facts,
                context,
                definition.name.span,
                SemanticKind::Definition,
                definition.name.value.clone(),
                Some(context.document.to_path_buf()),
            );
            visit_type(&definition.body, context, facts);
            visit_annotations(&definition.annotations, context, facts);
        }
        TopLevel::Constant(constant) => {
            push(
                facts,
                context,
                constant.name.span,
                SemanticKind::Definition,
                constant.name.value.clone(),
                Some(context.document.to_path_buf()),
            );
            visit_type(&constant.ty, context, facts);
            visit_expr(&constant.value, context, facts);
            visit_annotations(&constant.annotations, context, facts);
        }
        TopLevel::Function(function) => index_function(function, context, facts),
        TopLevel::Macro(macro_def) => {
            push(
                facts,
                context,
                macro_def.name.span,
                SemanticKind::Definition,
                macro_def.name.value.clone(),
                Some(context.document.to_path_buf()),
            );
            // Macro bodies are compiler-form CST over syntax categories, not
            // typed `Expr`/`TypeExpr` trees, and are fully expanded away by
            // the time `SurfaceProgram` exists. There is nothing further to
            // index here.
        }
        TopLevel::Test(test) => {
            push(
                facts,
                context,
                test.name.span,
                SemanticKind::Definition,
                test.name.value.clone(),
                Some(context.document.to_path_buf()),
            );
            for expr in &test.body {
                visit_expr(expr, context, facts);
            }
        }
        TopLevel::TestScenario(scenario) => {
            for test in &scenario.cases {
                push(
                    facts,
                    context,
                    test.name.span,
                    SemanticKind::Definition,
                    format!("{}::{}", scenario.name.value, test.name.value),
                    Some(context.document.to_path_buf()),
                );
                for expr in &test.body {
                    visit_expr(expr, context, facts);
                }
            }
        }
    }
}

fn index_function(
    function: &ast::Function,
    context: &DocContext<'_>,
    facts: &mut Vec<SemanticFact>,
) {
    push(
        facts,
        context,
        function.name.span,
        SemanticKind::Definition,
        function.name.value.clone(),
        Some(context.document.to_path_buf()),
    );
    for parameter in &function.parameters {
        visit_type(&parameter.ty, context, facts);
    }
    visit_type(&function.return_type, context, facts);
    for expr in &function.body {
        visit_expr(expr, context, facts);
    }
    visit_annotations(&function.annotations, context, facts);
}

fn visit_annotations(
    annotations: &[Annotation],
    context: &DocContext<'_>,
    facts: &mut Vec<SemanticFact>,
) {
    for annotation in annotations {
        match &annotation.value {
            AnnotationKind::Doc(_) => {}
            // Indexing the labels as types is what makes go-to-definition and
            // find-references work on an effect name inside `effects:`.
            AnnotationKind::Effects(row) => {
                for label in &row.labels {
                    visit_type(label, context, facts);
                }
            }
            AnnotationKind::Where(parameters) => {
                for parameter in parameters {
                    for bound in &parameter.bounds {
                        visit_type(bound, context, facts);
                    }
                }
            }
            AnnotationKind::Definitions(functions) => {
                for function in functions {
                    index_function(function, context, facts);
                }
            }
            AnnotationKind::Implementation { interface, items } => {
                visit_type(interface, context, facts);
                for item in items {
                    match item {
                        ImplItem::Types(types) => {
                            for ty in types {
                                visit_type(ty, context, facts);
                            }
                        }
                        ImplItem::Method { binding, .. } => match binding {
                            MethodBinding::Reference(name) => push(
                                facts,
                                context,
                                name.span,
                                SemanticKind::Reference,
                                name.value.clone(),
                                resolve_reference_target(&name.value, context),
                            ),
                            MethodBinding::Function(function) => {
                                index_function(function, context, facts)
                            }
                        },
                    }
                }
            }
        }
    }
}

fn visit_expr(expr: &Expr, context: &DocContext<'_>, facts: &mut Vec<SemanticFact>) {
    match &expr.value {
        ExprKind::Literal(Literal::Atom(name)) => push(
            facts,
            context,
            expr.span,
            SemanticKind::Atom,
            name.clone(),
            None,
        ),
        ExprKind::Literal(_) | ExprKind::Break | ExprKind::Continue => {}
        ExprKind::Reference(name) => push(
            facts,
            context,
            expr.span,
            SemanticKind::Reference,
            name.clone(),
            resolve_reference_target(name, context),
        ),
        ExprKind::Call {
            callee, arguments, ..
        } => {
            push(
                facts,
                context,
                callee.span,
                SemanticKind::Call,
                callee.value.clone(),
                resolve_reference_target(&callee.value, context),
            );
            for argument in arguments {
                visit_expr(argument.value(), context, facts);
            }
        }
        ExprKind::AnonymousFunction { body, .. } => visit_exprs(body, context, facts),
        ExprKind::Do(values) | ExprKind::Tuple(values) | ExprKind::Array(values) => {
            visit_exprs(values, context, facts)
        }
        ExprKind::Let { ty, value, .. } => {
            if let Some(ty) = ty {
                visit_type(ty, context, facts);
            }
            visit_expr(value, context, facts);
        }
        ExprKind::Set { value, .. } => visit_expr(value, context, facts),
        ExprKind::Return(value) => {
            if let Some(value) = value {
                visit_expr(value, context, facts);
            }
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            visit_expr(condition, context, facts);
            visit_exprs(then_body, context, facts);
            visit_exprs(else_body, context, facts);
        }
        ExprKind::While { condition, body } => {
            visit_expr(condition, context, facts);
            visit_exprs(body, context, facts);
        }
        ExprKind::For { source, body, .. } => {
            visit_expr(source, context, facts);
            visit_exprs(body, context, facts);
        }
        ExprKind::Match { target, cases } => {
            visit_expr(target, context, facts);
            for case in cases {
                visit_pattern(&case.pattern, context, facts);
                visit_exprs(&case.body, context, facts);
            }
        }
        ExprKind::Record(fields) => {
            for field in fields {
                visit_expr(&field.value, context, facts);
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                visit_expr(key, context, facts);
                visit_expr(value, context, facts);
            }
        }
        ExprKind::Mutable(value) | ExprKind::ReferenceOf(value) => {
            visit_expr(value, context, facts)
        }
        ExprKind::Range(start, end, step) => {
            visit_expr(start, context, facts);
            visit_expr(end, context, facts);
            visit_expr(step, context, facts);
        }
        ExprKind::Convert { value, into, .. } | ExprKind::Cast { value, into } => {
            visit_expr(value, context, facts);
            visit_type(into, context, facts);
        }
        ExprKind::Embed { .. } | ExprKind::Wasm { .. } => {}
        ExprKind::Template { bindings, .. } => {
            for binding in bindings {
                visit_expr(&binding.value, context, facts);
            }
        }
        ExprKind::Task { body, .. } => visit_exprs(body, context, facts),
        ExprKind::Spawn { value, .. } => visit_expr(value, context, facts),
        ExprKind::Join { .. } => {}
    }
}

fn visit_exprs(values: &[Expr], context: &DocContext<'_>, facts: &mut Vec<SemanticFact>) {
    for value in values {
        visit_expr(value, context, facts);
    }
}

fn visit_pattern(pattern: &Pattern, context: &DocContext<'_>, facts: &mut Vec<SemanticFact>) {
    match &pattern.value {
        PatternKind::Literal(Literal::Atom(name)) => push(
            facts,
            context,
            pattern.span,
            SemanticKind::Atom,
            name.clone(),
            None,
        ),
        PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Bind(_) => {}
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            push(
                facts,
                context,
                constructor.span,
                SemanticKind::Reference,
                constructor.value.clone(),
                resolve_reference_target(&constructor.value, context),
            );
            for argument in arguments {
                visit_pattern(argument, context, facts);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                visit_pattern(&field.pattern, context, facts);
            }
        }
        PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
            for pattern in patterns {
                visit_pattern(pattern, context, facts);
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                visit_pattern(key, context, facts);
                visit_pattern(value, context, facts);
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            visit_type(ty, context, facts);
            visit_pattern(pattern, context, facts);
        }
    }
}

fn visit_type(ty: &TypeExpr, context: &DocContext<'_>, facts: &mut Vec<SemanticFact>) {
    match &ty.value {
        TypeExprKind::Named(name) => push(
            facts,
            context,
            ty.span,
            SemanticKind::Reference,
            name.clone(),
            resolve_reference_target(name, context),
        ),
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            push(
                facts,
                context,
                constructor.span,
                SemanticKind::Reference,
                constructor.value.clone(),
                resolve_reference_target(&constructor.value, context),
            );
            for argument in arguments {
                visit_type(argument, context, facts);
            }
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                visit_type(&member.ty, context, facts);
            }
        }
        TypeExprKind::Tuple(types)
        | TypeExprKind::Union(types)
        | TypeExprKind::Intersect(types) => {
            for ty in types {
                visit_type(ty, context, facts);
            }
        }
        TypeExprKind::Array(inner)
        | TypeExprKind::Newtype(inner)
        | TypeExprKind::Mutable(inner)
        | TypeExprKind::Reference(inner)
        | TypeExprKind::MutableReference(inner) => visit_type(inner, context, facts),
        TypeExprKind::Map(key, value) => {
            visit_type(key, context, facts);
            visit_type(value, context, facts);
        }
        TypeExprKind::Function { parameters, result } => {
            for parameter in parameters {
                visit_type(&parameter.ty, context, facts);
            }
            visit_type(result, context, facts);
        }
        TypeExprKind::WasmValue(name) => push(
            facts,
            context,
            name.span,
            SemanticKind::Reference,
            name.value.clone(),
            resolve_reference_target(&name.value, context),
        ),
        TypeExprKind::Literal(Literal::Atom(name)) => push(
            facts,
            context,
            ty.span,
            SemanticKind::Atom,
            name.clone(),
            None,
        ),
        TypeExprKind::Effect { domain, action } => {
            for atom in [domain, action] {
                push(
                    facts,
                    context,
                    atom.span,
                    SemanticKind::Atom,
                    atom.value.clone(),
                    None,
                );
            }
        }
        TypeExprKind::Handle(_) | TypeExprKind::Literal(_) => {}
    }
}

/// Resolve a `Reference`/`Call` target module, when known precisely: a
/// qualified symbol whose first segment is a declared import alias resolves
/// to that alias's target module. An unqualified symbol is a same-module
/// reference; the exact defining document is not guessed here (query local
/// `Definition` facts to resolve it).
fn resolve_reference_target(symbol: &str, context: &DocContext<'_>) -> Option<PathBuf> {
    let (alias, _) = symbol.split_once('.')?;
    context.import_targets.get(alias).cloned()
}

fn push(
    facts: &mut Vec<SemanticFact>,
    context: &DocContext<'_>,
    span: Span,
    kind: SemanticKind,
    symbol: String,
    target_document: Option<PathBuf>,
) {
    facts.push(SemanticFact {
        document: context.document.to_path_buf(),
        document_id: context.document_id,
        span,
        kind,
        symbol,
        target_document,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
    }

    #[test]
    fn indexes_definitions_for_every_top_level_form() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(def widget (record (name str)))\n\
             (const answer int64 1)\n\
             (defn double (value int64) int64 (do value))\n\
             (test.scenario \"doubling\" (test.case \"doubling\" unit))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let mut names = index
            .query(Some(SemanticKind::Definition), None)
            .into_iter()
            .map(|fact| fact.symbol.as_str())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["answer", "double", "doubling::doubling", "widget"]);
    }

    #[test]
    fn indexes_imports_with_resolved_targets() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(import helper \"helper.vibra\")\n");
        write(
            &temp.path().join("helper.vibra"),
            "(defn run () void (do unit))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let imports = index.query(Some(SemanticKind::Import), Some("helper"));
        assert_eq!(imports.len(), 1);
        let helper = fs::canonicalize(temp.path().join("helper.vibra")).unwrap();
        assert_eq!(imports[0].target_document, Some(helper));
    }

    #[test]
    fn indexes_references_and_calls_in_function_bodies() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(defn helper () int64 (do 1))\n\
             (defn main () int64 (do (helper)))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let calls = index.query(Some(SemanticKind::Call), Some("helper"));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].symbol, "helper");

        let source = fs::read_to_string(&entry).unwrap();
        let call_span = calls[0].span;
        assert_eq!(&source[call_span.range()], "helper");
    }

    #[test]
    fn spans_are_exact_half_open_byte_ranges_including_unicode() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        // Symbols are ASCII-only per the reader grammar, but comments and
        // string literals are not: `λ` (2 UTF-8 bytes) and `😀` (4 UTF-8
        // bytes) both precede the indexed call, so a byte-vs-codepoint
        // counting bug in either the lexer or this module would shift the
        // recorded span away from the real occurrence of `helper`.
        let source = "; λ comment\n\
                       (defn helper () str (do \"unicode: \u{3bb} \u{1f600}\"))\n\
                       (defn main () str (do (helper)))\n";
        write(&entry, source);
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let calls = index.query(Some(SemanticKind::Call), Some("helper"));
        assert_eq!(calls.len(), 1);
        let span = calls[0].span;
        assert_eq!(&source[span.range()], "helper");
        assert_eq!(span.start, source.rfind("helper").unwrap());
        // The span is a half-open byte range: `end` is exactly one past the
        // last byte of the symbol (`r`), and that byte is excluded from the
        // range, not included.
        assert_eq!(source.as_bytes()[span.end - 1], b'r');
        assert_ne!(source.as_bytes().get(span.end), Some(&b'r'));
    }

    #[test]
    fn facts_from_different_documents_with_identical_offsets_do_not_collide() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        let helper_a = temp.path().join("a.vibra");
        let helper_b = temp.path().join("b.vibra");
        // Both `a.vibra` and `b.vibra` place a function named `same-shape`
        // at the identical byte offset (0). If facts were keyed only by
        // span, these two definitions would be indistinguishable. Keying by
        // `(document_id, span)` — the whole point of PR #161's
        // document-qualified identities — keeps them apart.
        write(&helper_a, "(defn same-shape () int64 (do 1))\n");
        write(&helper_b, "(defn same-shape () int64 (do 2))\n");
        write(
            &entry,
            "(import a \"a.vibra\")\n(import b \"b.vibra\")\n(defn main () int64 (do (a.same-shape)))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let definitions = index.query(Some(SemanticKind::Definition), Some("same-shape"));
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].span, definitions[1].span);
        assert_ne!(definitions[0].document_id, definitions[1].document_id);
        assert_ne!(definitions[0].document, definitions[1].document);

        let a_id = ast::DocumentId::from_path(&fs::canonicalize(&helper_a).unwrap());
        let b_id = ast::DocumentId::from_path(&fs::canonicalize(&helper_b).unwrap());
        assert_eq!(
            index.document_path(a_id),
            Some(fs::canonicalize(&helper_a).unwrap().as_path())
        );
        assert_eq!(
            index.document_path(b_id),
            Some(fs::canonicalize(&helper_b).unwrap().as_path())
        );
        assert!(definitions.iter().any(|fact| fact.document_id == a_id));
        assert!(definitions.iter().any(|fact| fact.document_id == b_id));
    }

    #[test]
    fn queries_the_lsp_relies_on_return_expected_shapes() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import util \"util.vibra\")\n\
             (defn main () str (do (util.greet)))\n",
        );
        write(
            &temp.path().join("util.vibra"),
            "(defn greet () str (do \"hi\"))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();

        // The LSP's `resolve()` looks up `query(Some(Import), Some(alias))`
        // filtered to the current document.
        let util = index.query(Some(SemanticKind::Import), Some("util"));
        assert_eq!(util.len(), 1);
        assert_eq!(util[0].document, fs::canonicalize(&entry).unwrap());
        let util_target = fs::canonicalize(temp.path().join("util.vibra")).unwrap();
        assert_eq!(util[0].target_document, Some(util_target));

        // `visible_symbols`/`visible_imports` enumerate every import in a
        // document with `query(Some(Import), None)`.
        let all_imports = index.query(Some(SemanticKind::Import), None);
        assert_eq!(all_imports.len(), 1);
        assert_eq!(all_imports[0].symbol, "util");

        let calls = index.query(Some(SemanticKind::Call), Some("util.greet"));
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn qualified_calls_resolve_target_document_through_the_import_alias() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import util \"util.vibra\")\n(defn main () str (do (util.greet)))\n",
        );
        write(
            &temp.path().join("util.vibra"),
            "(defn greet () str (do \"hi\"))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let calls = index.query(Some(SemanticKind::Call), Some("util.greet"));
        let util_target = fs::canonicalize(temp.path().join("util.vibra")).unwrap();
        assert_eq!(calls[0].target_document, Some(util_target));
    }

    #[test]
    fn unqualified_references_leave_target_document_unresolved() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(defn helper () int64 (do 1))\n(defn main () int64 (do (helper)))\n",
        );
        let index = SemanticIndex::build(&entry, &CompilationFlags::default()).unwrap();
        let calls = index.query(Some(SemanticKind::Call), Some("helper"));
        assert_eq!(calls[0].target_document, None);
    }
}
