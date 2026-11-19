//! The seven top-level forms and the checks of this step.
//!
//! `docs/spec/01-source-language.md`, "Declarations": Vibra has exactly seven
//! top-level forms. `use` is complete here (`use atom [as identifier]
//! enduse`); `type`, `interface`, and the module value `let` have their own
//! modules; `fn`, `proc`, and `test` are generic constructs whose bodies are
//! reported `@tool.unavailable` until their own grammar exists. The generic section rules of "Grammar"
//! apply to every construct: a repeated section is
//! `@syntax.duplicateSection` at the repeated keyword; in a `fn`, `proc`, or
//! `test`, `do` comes last; a declaration's name comes before its sections;
//! `as` outside a `use` is `@syntax.invalidForm` at the `as`; and an `impl`
//! that is not a direct child of a `type` or an `interface` is
//! `@syntax.invalidForm` over the `impl`.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::construct::is_trivia;
use super::interface_decl::InterfaceDecl;
use super::keyword::BODY_CONSTRUCTS;
use super::section::{Construct, Section};
use super::statement::ModuleLet;
use super::type_decl::TypeDecl;
use crate::cst::{CstNode, SyntaxKind};

/// One top-level form of a module.
#[derive(Clone, Copy, Debug)]
pub enum Declaration<'a> {
    /// `use … enduse`.
    Use(Use<'a>),
    /// `type … endtype`.
    Type(TypeDecl<'a>),
    /// `interface … endinterface`.
    Interface(InterfaceDecl<'a>),
    /// `fn … endfn`.
    Fn(Construct<'a>),
    /// `proc … endproc`.
    Proc(Construct<'a>),
    /// `let … endlet`, a module value.
    Let(ModuleLet<'a>),
    /// `test … endtest`.
    Test(Construct<'a>),
}

impl<'a> Declaration<'a> {
    /// The view of a top-level node of the document `text`, when it is one
    /// of the seven forms.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        let construct = Construct::new(node, text)?;
        Some(match node.kind() {
            SyntaxKind::Use => Self::Use(Use::new(construct, text)),
            SyntaxKind::TypeDecl => Self::Type(TypeDecl::new(node, text)?),
            SyntaxKind::InterfaceDecl => {
                Self::Interface(InterfaceDecl::new(node, text)?)
            }
            SyntaxKind::FnDecl => Self::Fn(construct),
            SyntaxKind::ProcDecl => Self::Proc(construct),
            SyntaxKind::ModuleLet => Self::Let(ModuleLet::new(node, text)?),
            SyntaxKind::Test => Self::Test(construct),
            _ => return None,
        })
    }

    /// The generic view of the form.
    #[must_use]
    pub fn construct(&self) -> Construct<'a> {
        match self {
            Self::Use(view) => view.construct,
            Self::Type(view) => view.construct(),
            Self::Interface(view) => view.construct(),
            Self::Let(view) => view.construct(),
            Self::Fn(construct) | Self::Proc(construct) | Self::Test(construct) => {
                *construct
            }
        }
    }
}

/// An atom as written: `@` and dot-separated parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Atom<'a> {
    text: &'a str,
    span: ByteSpan,
}

impl<'a> Atom<'a> {
    /// The view of an atom leaf of the document `text`.
    pub(crate) fn of(node: &CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Atom).then(|| Self {
            text: node.text(text),
            span: node.span(),
        })
    }

    /// The atom's text, `@` included.
    #[must_use]
    pub const fn text(&self) -> &'a str {
        self.text
    }

    /// The atom's bytes.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The parts, with their exact case.
    #[must_use]
    pub fn parts(&self) -> Vec<&'a str> {
        self.text
            .strip_prefix('@')
            .map(|parts| parts.split('.').collect())
            .unwrap_or_default()
    }
}

/// A name as written. A reserved word recovered in a name position is a
/// name too, so that the error does not cascade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ident<'a> {
    text: &'a str,
    span: ByteSpan,
    reserved: bool,
}

impl<'a> Ident<'a> {
    /// The name.
    #[must_use]
    pub const fn text(&self) -> &'a str {
        self.text
    }

    /// The name's bytes.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Whether the name is a reserved word, recovered.
    #[must_use]
    pub const fn is_reserved(&self) -> bool {
        self.reserved
    }
}

/// `use atom [as identifier] enduse`.
#[derive(Clone, Copy, Debug)]
pub struct Use<'a> {
    construct: Construct<'a>,
    locator: Option<Atom<'a>>,
    alias: Option<Ident<'a>>,
}

impl<'a> Use<'a> {
    fn new(construct: Construct<'a>, text: &'a str) -> Self {
        let locator = construct
            .head()
            .first()
            .filter(|node| node.kind() == SyntaxKind::Atom)
            .map(|node| Atom {
                text: node.text(text),
                span: node.span(),
            });
        let alias = construct
            .sections()
            .into_iter()
            .find(|section| section.keyword() == "as")
            .and_then(|section| match section.operand().as_slice() {
                [name] => name_of(name, text),
                _ => None,
            });
        Self {
            construct,
            locator,
            alias,
        }
    }

    /// The generic view.
    #[must_use]
    pub const fn construct(&self) -> Construct<'a> {
        self.construct
    }

    /// The locator, when the first token after `use` is an atom.
    #[must_use]
    pub const fn locator(&self) -> Option<Atom<'a>> {
        self.locator
    }

    /// The name after `as`, when one is written.
    #[must_use]
    pub const fn alias(&self) -> Option<Ident<'a>> {
        self.alias
    }

    /// The local name: the alias, or the locator's last part.
    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.alias
            .map(|alias| alias.text())
            .or_else(|| self.locator.and_then(|locator| locator.parts().pop()))
    }
}

/// The name a leaf spells: an identifier, or a reserved word recovered as a
/// name.
pub(crate) fn name_of<'a>(node: &CstNode, text: &'a str) -> Option<Ident<'a>> {
    let reserved = match node.kind() {
        SyntaxKind::Identifier => false,
        SyntaxKind::ReservedWord => true,
        _ => return None,
    };
    Some(Ident {
        text: node.text(text),
        span: node.span(),
        reserved,
    })
}

/// Checks a top-level `use`: its locator, its one section `as` with its one
/// name, and nothing else. Recovered tokens (`@error` nodes, a missing
/// closer) already have their own diagnostic and are not reported again.
pub(crate) fn check_use(node: &CstNode, text: &str, diagnostics: &mut Vec<Diagnostic>) {
    let Some(construct) = Construct::new(node, text) else {
        return;
    };
    let contents: Vec<&CstNode> = construct
        .contents()
        .iter()
        .filter(|child| !is_trivia(child) && child.kind() != SyntaxKind::Error)
        .collect();
    let mut invalid = false;
    let mut rest = contents.as_slice();
    match rest.split_first() {
        Some((locator, after)) if locator.kind() == SyntaxKind::Atom => rest = after,
        _ => invalid = true,
    }
    let mut seen_as = false;
    for item in rest {
        let Some(section) = Section::new(item, text) else {
            invalid = true;
            continue;
        };
        if section.keyword() != "as" {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidSection,
                section.span(),
                "a `use` has no section but `as`",
            ));
            continue;
        }
        if std::mem::replace(&mut seen_as, true) {
            // The repeat is `@syntax.duplicateSection`, reported once.
            continue;
        }
        let operand: Vec<&CstNode> = section
            .operand()
            .into_iter()
            .filter(|node| node.kind() != SyntaxKind::Error)
            .collect();
        match operand.as_slice() {
            [name] if name.kind() == SyntaxKind::Identifier => {}
            [name] if name.kind() == SyntaxKind::ReservedWord => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::NameReservedDeclaration,
                    name.span(),
                    "a reserved word cannot be an import name",
                ));
            }
            _ => invalid = true,
        }
    }
    if invalid {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            node.span(),
            "a `use` is `use`, one atom, an optional `as` and one name, and `enduse`",
        ));
    }
}

/// Applies the generic section rules to every construct in `node`, a
/// top-level form: repeated sections, `do` last, the name first, `as`
/// outside a `use`, and an `impl` outside a `type` or `interface`. The walk
/// is iterative.
pub(crate) fn check_sections(
    node: &CstNode,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(construct) = Construct::new(node, text)
        && construct.keyword() != "use"
        && construct
            .contents()
            .iter()
            .find(|child| !is_trivia(child))
            .is_some_and(|first| first.kind() == SyntaxKind::Section)
    {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            node.span(),
            "a declaration's name comes before its sections",
        ));
    }
    // Each entry: a node, and the keyword of the construct it is directly in.
    let mut stack: Vec<(&CstNode, &str)> = vec![(node, "")];
    while let Some((current, parent)) = stack.pop() {
        if current.kind() == SyntaxKind::Section {
            let mut children = current.children().iter();
            if let Some(keyword) = children.next()
                && parent != "use"
            {
                report_as(keyword, text, diagnostics);
            }
            stack.extend(children.map(|child| (child, "")));
            continue;
        }
        if let Some(construct) = Construct::new(current, text) {
            check_construct(construct, diagnostics);
            let keyword = construct.keyword();
            if keyword == "impl" && !matches!(parent, "type" | "interface") {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::SyntaxInvalidForm,
                    construct.span(),
                    "an `impl` is written only directly in a `type` or an `interface`",
                ));
            }
            stack.extend(
                current
                    .children()
                    .iter()
                    .skip(1)
                    .map(|child| (child, keyword)),
            );
            continue;
        }
        if current.kind().is_leaf() {
            report_as(current, text, diagnostics);
        } else {
            stack.extend(current.children().iter().map(|child| (child, "")));
        }
    }
}

fn report_as(leaf: &CstNode, text: &str, diagnostics: &mut Vec<Diagnostic>) {
    if leaf.kind() == SyntaxKind::ReservedWord && leaf.text(text) == "as" {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            leaf.span(),
            "`as` is written only in a `use`",
        ));
    }
}

/// Repeated sections, and `do` last in a `fn`, `proc`, or `test`.
pub(crate) fn check_construct(
    construct: Construct<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: Vec<&str> = Vec::new();
    let mut after_do = false;
    let mut misplaced = false;
    let has_body = BODY_CONSTRUCTS.contains(&construct.keyword());
    for section in construct.sections() {
        let keyword = section.keyword();
        if seen.contains(&keyword) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxDuplicateSection,
                section.keyword_span(),
                "every section occurs at most once",
            ));
            continue;
        }
        seen.push(keyword);
        if has_body && after_do {
            misplaced = true;
        }
        after_do |= keyword == "do";
    }
    if misplaced {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            construct.span(),
            "`do` and its body come last",
        ));
    }
}
