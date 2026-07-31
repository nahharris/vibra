use std::fmt;

use super::{lex, Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub nodes: Vec<Node>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    List(Vec<Node>),
    Atom(Atom),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Atom {
    Symbol(String),
    Label(String),
    Atom(String),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl SyntaxError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}: {}",
            self.code, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for SyntaxError {}

pub fn parse(source: &str) -> Result<Document, SyntaxError> {
    Parser::new(lex(source)?, source.len()).parse()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    source_len: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, source_len: usize) -> Self {
        Self {
            tokens,
            cursor: 0,
            source_len,
        }
    }

    fn parse(mut self) -> Result<Document, SyntaxError> {
        let mut nodes = Vec::new();
        while self.cursor < self.tokens.len() {
            nodes.push(self.node()?);
        }
        Ok(Document {
            nodes,
            span: Span::new(0, self.source_len),
        })
    }

    fn node(&mut self) -> Result<Node, SyntaxError> {
        let token = self.tokens[self.cursor].clone();
        self.cursor += 1;
        let kind = match token.kind {
            TokenKind::OpenParen => return self.list(token.span),
            TokenKind::CloseParen => {
                return Err(SyntaxError::new(
                    "E-SYN-005",
                    "unexpected closing parenthesis",
                    token.span,
                ));
            }
            TokenKind::Symbol(value) => NodeKind::Atom(Atom::Symbol(value)),
            TokenKind::Label(value) => NodeKind::Atom(Atom::Label(value)),
            TokenKind::Atom(value) => NodeKind::Atom(Atom::Atom(value)),
            TokenKind::String(value) => NodeKind::Atom(Atom::String(value)),
            TokenKind::Int(value) => NodeKind::Atom(Atom::Int(value)),
            TokenKind::Float(value) => NodeKind::Atom(Atom::Float(value)),
            TokenKind::Bool(value) => NodeKind::Atom(Atom::Bool(value)),
            TokenKind::Unit => NodeKind::Atom(Atom::Unit),
            TokenKind::Comment(value) => NodeKind::Comment(value),
        };
        Ok(Node {
            kind,
            span: token.span,
        })
    }

    fn list(&mut self, open_span: Span) -> Result<Node, SyntaxError> {
        let mut children = Vec::new();
        loop {
            let Some(token) = self.tokens.get(self.cursor) else {
                return Err(SyntaxError::new(
                    "E-SYN-006",
                    "unclosed list; expected `)`",
                    Span::new(open_span.start, self.source_len),
                ));
            };
            if token.kind == TokenKind::CloseParen {
                let close_span = token.span;
                self.cursor += 1;
                return Ok(Node {
                    kind: NodeKind::List(children),
                    span: open_span.cover(close_span),
                });
            }
            children.push(self.node()?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_lists_comments_and_multiple_roots() {
        let document = parse("(module ; reader note\n (def answer 42))\nunit").unwrap();
        assert_eq!(document.nodes.len(), 2);
        let NodeKind::List(module) = &document.nodes[0].kind else {
            panic!("expected module list");
        };
        assert_eq!(module.len(), 3);
        assert!(matches!(module[1].kind, NodeKind::Comment(_)));
        assert_eq!(document.nodes[0].span.range(), 0..39);
        assert!(matches!(document.nodes[1].kind, NodeKind::Atom(Atom::Unit)));
    }

    #[test]
    fn preserves_labels_as_structural_atoms() {
        let document = parse("(def option doc: \"Maybe\")").unwrap();
        let NodeKind::List(children) = &document.nodes[0].kind else {
            panic!("expected list");
        };
        assert!(matches!(
            &children[2].kind,
            NodeKind::Atom(Atom::Label(label)) if label == "doc"
        ));
        assert_eq!(children[2].span, Span::new(12, 16));
    }

    #[test]
    fn preserves_atom_literals() {
        let document = parse("@http.not-found").unwrap();
        assert!(matches!(
            &document.nodes[0].kind,
            NodeKind::Atom(Atom::Atom(name)) if name == "http.not-found"
        ));
    }

    #[test]
    fn empty_list_is_a_list_not_unit() {
        let document = parse("() unit").unwrap();
        assert!(matches!(document.nodes[0].kind, NodeKind::List(ref v) if v.is_empty()));
        assert!(matches!(document.nodes[1].kind, NodeKind::Atom(Atom::Unit)));
    }

    #[test]
    fn reports_parenthesis_errors_with_stable_codes() {
        let error = parse(")").unwrap_err();
        assert_eq!(error.code, "E-SYN-005");
        assert_eq!(error.span, Span::new(0, 1));

        let error = parse("(a (b)").unwrap_err();
        assert_eq!(error.code, "E-SYN-006");
        assert_eq!(error.span, Span::new(0, 6));
    }
}
