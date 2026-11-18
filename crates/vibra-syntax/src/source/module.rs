//! The module: `module = { top }, EOF`.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Exclusions": a token at
//! the top level that does not open one of the seven top-level forms is
//! `@syntax.invalidForm`, once per run of such tokens, over the run; the run
//! is kept in an `@error` node and recovery resumes at the next top-level
//! opener. A run holds balanced constructs and brackets whole, so a `(`
//! where a top-level form is expected is reported once over the balanced
//! parenthesized form, and an `as` alone is reported at the `as`. Every
//! `use` precedes every other top-level form; a later `use` is
//! `@syntax.invalidForm` at its keyword.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::construct::is_trivia;
use super::declaration::{Declaration, check_construct, check_sections, check_use};
use super::section::Construct;
use crate::cst::{CstNode, SyntaxKind};
use crate::document::{Document, DocumentMode};

/// Whether `kind` is one of the seven top-level forms.
const fn is_top_form(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Use
            | SyntaxKind::TypeDecl
            | SyntaxKind::InterfaceDecl
            | SyntaxKind::FnDecl
            | SyntaxKind::ProcDecl
            | SyntaxKind::ModuleLet
            | SyntaxKind::Test
    )
}

/// Wraps the top-level items in the `@source` document node and reports the
/// top-level faults.
pub(crate) fn shape_module(
    items: Vec<CstNode>,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> CstNode {
    let mut out: Vec<CstNode> = Vec::with_capacity(items.len());
    let mut run: Vec<CstNode> = Vec::new();
    let mut after_other_form = false;
    for item in items {
        if is_top_form(item.kind()) {
            flush_run(&mut run, &mut out, diagnostics);
            check_form(&item, text, after_other_form, diagnostics);
            after_other_form |= item.kind() != SyntaxKind::Use;
            out.push(item);
        } else if run.is_empty() && (is_trivia(&item) || item.kind() == SyntaxKind::Error) {
            out.push(item);
        } else {
            run.push(item);
        }
    }
    flush_run(&mut run, &mut out, diagnostics);
    CstNode::interior(SyntaxKind::Source, out, 0)
}

fn check_form(
    item: &CstNode,
    text: &str,
    after_other_form: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item.kind() == SyntaxKind::Use {
        if let Some(construct) = Construct::new(item, text) {
            check_construct(construct, diagnostics);
            if after_other_form {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::SyntaxInvalidForm,
                    construct.keyword_span(),
                    "every `use` precedes every other top-level form",
                ));
            }
        }
        check_use(item, text, diagnostics);
        return;
    }
    check_sections(item, text, diagnostics);
    diagnostics.push(Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        item.span(),
        "the grammar of this form is not available in this reader",
    ));
}

/// Ends a run of tokens that open no top-level form: its trailing trivia
/// stays outside, the rest goes into one `@error` node, and one
/// `@syntax.invalidForm` covers it.
fn flush_run(run: &mut Vec<CstNode>, out: &mut Vec<CstNode>, diagnostics: &mut Vec<Diagnostic>) {
    if run.is_empty() {
        return;
    }
    let keep = run
        .iter()
        .rposition(|item| !is_trivia(item))
        .map_or(0, |last| last + 1);
    let trivia = run.split_off(keep);
    let items = std::mem::take(run);
    let start = items.first().map_or(0, |item| item.span().start());
    let end = items
        .iter()
        .rev()
        .find(|item| item.kind() != SyntaxKind::Error && !is_trivia(item))
        .map_or(start, |item| item.span().end());
    diagnostics.push(Diagnostic::new(
        DiagnosticCode::SyntaxInvalidForm,
        ByteSpan::new(start, end),
        "only `use`, `type`, `interface`, `fn`, `proc`, `let`, and `test` are top-level forms",
    ));
    out.push(CstNode::interior(SyntaxKind::Error, items, start));
    out.extend(trivia);
}

/// The typed view of a source document's module.
#[derive(Clone, Copy, Debug)]
pub struct Module<'a> {
    root: &'a CstNode,
    text: &'a str,
}

impl<'a> Module<'a> {
    /// The document node.
    #[must_use]
    pub const fn root(&self) -> &'a CstNode {
        self.root
    }

    /// The document text.
    #[must_use]
    pub const fn text(&self) -> &'a str {
        self.text
    }

    /// The top-level forms, in written order. Tokens recovery kept in an
    /// `@error` node are not forms.
    #[must_use]
    pub fn declarations(&self) -> Vec<Declaration<'a>> {
        self.root
            .children()
            .iter()
            .filter_map(|child| Declaration::new(child, self.text))
            .collect()
    }

    /// The imports, in written order.
    #[must_use]
    pub fn uses(&self) -> Vec<super::declaration::Use<'a>> {
        self.declarations()
            .into_iter()
            .filter_map(|declaration| match declaration {
                Declaration::Use(view) => Some(view),
                _ => None,
            })
            .collect()
    }
}

/// The module of a source document, or `None` when the document is not a
/// source document or has no tree.
#[must_use]
pub fn module(document: &Document) -> Option<Module<'_>> {
    if document.mode() != DocumentMode::Source {
        return None;
    }
    Some(Module {
        root: document.root()?,
        text: document.text()?,
    })
}
