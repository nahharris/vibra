//! The typed view of `test … endtest`.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "`test`":
//!
//! ```text
//! test        = "test", string, [ "takes", params ], [ performs ],
//!               "do", body, "endtest" ;
//! ```
//!
//! A `@test` node holds its name string, `takes` and its `@params` node,
//! its `@performs` node, and `do` and its `@body`. `takes` admits exactly
//! one parameter; the shaping is in the source shaper.

use super::callable::{params_of, performs_row};
use super::construct::is_trivia;
use super::pattern::Pattern;
use super::section::Construct;
use super::statement::Body;
use super::types::{Row, TypeExpr};
use crate::cst::{CstNode, SyntaxKind};

/// `test "name" [takes {world: TestWorld}] [performs {…}] do … endtest`.
#[derive(Clone, Copy, Debug)]
pub struct Test<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Test<'a> {
    /// The view of a `@test` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Test).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The generic construct view.
    #[must_use]
    pub const fn construct(&self) -> Construct<'a> {
        Construct::of_declaration(self.node, self.text)
    }

    /// The name string as written, quotes included.
    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.construct()
            .contents()
            .iter()
            .find(|child| !is_trivia(child))
            .filter(|first| first.kind() == SyntaxKind::String)
            .map(|name| name.text(self.text))
    }

    /// The name's value, its escapes decoded.
    #[must_use]
    pub fn name_value(&self) -> Option<String> {
        self.name().and_then(crate::literal::decode_string)
    }

    /// The parameter of `takes`, when it has exactly one.
    #[must_use]
    pub fn takes(&self) -> Option<(Pattern<'a>, TypeExpr<'a>)> {
        let params = self
            .construct()
            .contents()
            .iter()
            .find(|child| child.kind() == SyntaxKind::Params)?;
        match params_of(params, self.text).as_slice() {
            [param] => Some(*param),
            _ => None,
        }
    }

    /// The `performs` row.
    #[must_use]
    pub fn performs(&self) -> Option<Row<'a>> {
        performs_row(self.node, self.text)
    }

    /// The body after `do`.
    #[must_use]
    pub fn body(&self) -> Option<Body<'a>> {
        self.construct()
            .contents()
            .iter()
            .find(|child| child.kind() == SyntaxKind::Body)
            .map(|node| Body::of(node, self.text))
    }
}
