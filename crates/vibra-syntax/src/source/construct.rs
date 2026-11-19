//! The construct tree: keyword-led constructs, sections, and brackets.
//!
//! `docs/spec/01-source-language.md`, "Design of the surface": every
//! construct opens with a keyword and ends with the closer named after it; a
//! section is a keyword followed by its operand. This module builds that
//! structure generically, with no knowledge of any one form:
//!
//! - an opener keyword ([`super::keyword::CONSTRUCTS`]) opens a construct
//!   closed by its typed closer, and a bracket opens a bracket group;
//! - inside a construct, a section keyword starts a section whose operand runs
//!   to the next section keyword, closer, or nested opener at the same depth;
//!   `do` at a section boundary directly inside a `fn`, `proc`, or `test` is
//!   the body section, which runs to the next section keyword or the closer
//!   and holds the constructs nested in it; a `do` that starts an expression
//!   (the operand of `requires`, `ensures`, `where`, or `be`, a guard's
//!   `else` consequence, or the operand of a prefix word) or a `do` inside
//!   the body opens a block closed by `enddo`;
//! - the word after `as`, the first word of a `type`, an `interface`, or a
//!   `let`, and the word after `case` in a `match` are name positions: a
//!   reserved word there other than the construct's own closer is taken as
//!   the name or binder;
//! - a reserved word followed by `:` directly in a bracket group, and an
//!   opener or a section keyword followed by `:` anywhere, is a label, a
//!   binder, or a result name, which the shaper recovers and reports: no
//!   opener or section keyword is ever followed by `:`;
//! - a closer is resolved by kind (`docs/spec/07-diagnostics-and-conformance.md`,
//!   "Recovery"), over constructs and brackets alike.
//!
//! A section's trailing whitespace and comments belong to its construct, and
//! a construct closed as missing ends at its last written token, so trivia
//! never sits inside a node that does not need it. The matcher is iterative.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::keyword::{
    BODY_CONSTRUCTS, TOP_LEVEL, closer_of, is_section, opener_keyword, opener_of,
};

/// The constructs whose first word is always a name position: a `type` or an
/// `interface` is never anonymous, and a `let` starts with its name or
/// pattern, so a reserved word written there is its recovered name or
/// binder, not a section or an opener.
const NAMED_CONSTRUCTS: &[&str] = &["type", "interface", "let"];

/// The words after which an expression is expected: the keywords of the
/// sections whose operand is an expression, the `else` that introduces a
/// guard's consequence, and the prefix words of `expr` and `unary`.
const EXPECTS_EXPRESSION: &[&str] = &[
    "requires", "ensures", "where", "be", "else", "return", "try", "await",
];
use crate::cst::{CstNode, SyntaxKind};
use crate::lexer::{Token, TokenKind};

/// What an open frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    /// A bracket group, by its opening token.
    Group(TokenKind),
    /// A keyword-led construct, by its opener keyword.
    Construct(&'static str),
}

/// One open construct or bracket group.
struct Frame {
    kind: FrameKind,
    /// The opener token.
    opener: ByteSpan,
    /// Whether it was opened at the top level of the document.
    top_level: bool,
    /// The items written so far, outside the open section.
    items: Vec<CstNode>,
    /// The open section: its keyword first, then its operand.
    section: Option<Vec<CstNode>>,
    /// Whether the open section is the body section `do`.
    body: bool,
    /// Whether the next token is a name position.
    expect_name: bool,
}

impl Frame {
    fn target(&mut self) -> &mut Vec<CstNode> {
        match &mut self.section {
            Some(section) => section,
            None => &mut self.items,
        }
    }

    /// Closes the open section, if any; its trailing trivia stays in the
    /// frame, after the section.
    fn close_section(&mut self) {
        let Some(mut section) = self.section.take() else {
            return;
        };
        self.body = false;
        let trivia = split_trailing_trivia(&mut section);
        let at = section.first().map_or(0, |item| item.span().start());
        self.items
            .push(CstNode::interior(SyntaxKind::Section, section, at));
        self.items.extend(trivia);
    }

    fn opener_word(&self) -> Option<&'static str> {
        match self.kind {
            FrameKind::Construct(word) => Some(word),
            FrameKind::Group(_) => None,
        }
    }
}

/// Removes the whitespace and comments at the end of `items` and returns
/// them, in order.
fn split_trailing_trivia(items: &mut Vec<CstNode>) -> Vec<CstNode> {
    let keep = items
        .iter()
        .rposition(|item| !is_trivia(item))
        .map_or(0, |last| last + 1);
    items.split_off(keep)
}

/// Whether a node is a whitespace or comment leaf.
pub(crate) fn is_trivia(node: &CstNode) -> bool {
    matches!(node.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
}

/// The node kind of a closed construct: a top-level form opened at the top
/// level, otherwise a generic construct.
fn construct_kind(opener: &str, top_level: bool) -> SyntaxKind {
    if !top_level || !TOP_LEVEL.contains(&opener) {
        return SyntaxKind::Construct;
    }
    match opener {
        "use" => SyntaxKind::Use,
        "type" => SyntaxKind::TypeDecl,
        "interface" => SyntaxKind::InterfaceDecl,
        "fn" => SyntaxKind::FnDecl,
        "proc" => SyntaxKind::ProcDecl,
        "let" => SyntaxKind::ModuleLet,
        _ => SyntaxKind::Test,
    }
}

const fn group_kind(opener: TokenKind) -> SyntaxKind {
    match opener {
        TokenKind::OpenBrace => SyntaxKind::Brace,
        TokenKind::OpenBracket => SyntaxKind::Bracket,
        _ => SyntaxKind::Paren,
    }
}

const fn group_opened_by(closer: TokenKind) -> Option<TokenKind> {
    match closer {
        TokenKind::CloseBrace => Some(TokenKind::OpenBrace),
        TokenKind::CloseBracket => Some(TokenKind::OpenBracket),
        TokenKind::CloseParen => Some(TokenKind::OpenParen),
        _ => None,
    }
}

/// The matcher's state.
struct Matcher<'a> {
    text: &'a str,
    top: Vec<CstNode>,
    stack: Vec<Frame>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Matcher<'_> {
    fn target(&mut self) -> &mut Vec<CstNode> {
        match self.stack.last_mut() {
            Some(frame) => frame.target(),
            None => &mut self.top,
        }
    }

    fn leaf(&mut self, token: Token) {
        let leaf = CstNode::leaf(SyntaxKind::of_token(token.kind), token.span);
        self.target().push(leaf);
    }

    fn open(&mut self, kind: FrameKind, token: Token) {
        let top_level = self.stack.is_empty();
        let expect_name = matches!(kind, FrameKind::Construct(opener) if NAMED_CONSTRUCTS.contains(&opener));
        self.stack.push(Frame {
            kind,
            opener: token.span,
            top_level,
            items: vec![CstNode::leaf(SyntaxKind::of_token(token.kind), token.span)],
            section: None,
            body: false,
            expect_name,
        });
    }

    /// Reads one token; `colon` says whether the next significant token is a
    /// `:`.
    fn token(&mut self, token: Token, colon: bool) {
        if token.kind.is_trivia() {
            self.leaf(token);
            return;
        }
        let word = if token.kind == TokenKind::ReservedWord {
            token.span.text(self.text).unwrap_or_default()
        } else {
            ""
        };
        if let Some(frame) = self.stack.last_mut()
            && std::mem::take(&mut frame.expect_name)
            && token.kind == TokenKind::ReservedWord
            && frame.opener_word().and_then(closer_of) != Some(word)
        {
            self.leaf(token);
            return;
        }
        if token.kind == TokenKind::ReservedWord
            && colon
            && self.names_before_colon(word)
        {
            self.leaf(token);
            return;
        }
        match token.kind {
            TokenKind::OpenBrace | TokenKind::OpenBracket | TokenKind::OpenParen => {
                self.open(FrameKind::Group(token.kind), token);
            }
            TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen => {
                let opener = group_opened_by(token.kind);
                self.close_by_kind(token, |kind| {
                    opener.is_some_and(|opener| kind == FrameKind::Group(opener))
                });
            }
            TokenKind::ReservedWord => self.reserved(token, word),
            _ => self.leaf(token),
        }
    }

    fn reserved(&mut self, token: Token, word: &str) {
        if let Some(opener) = opener_of(word) {
            self.close_by_kind(token, |kind| kind == FrameKind::Construct(opener));
            return;
        }
        let in_construct = self.stack.last().and_then(|frame| {
            let opener = frame.opener_word()?;
            Some((opener, BODY_CONSTRUCTS.contains(&opener) && !frame.body))
        });
        if word == "do"
            && let Some((_, true)) = in_construct
            && !self.expects_operand()
        {
            self.start_section(token, true);
            return;
        }
        if let Some(opener) = opener_keyword(word) {
            if let Some(frame) = self.stack.last_mut()
                && !frame.body
            {
                frame.close_section();
            }
            self.open(FrameKind::Construct(opener), token);
            return;
        }
        if is_section(word) && in_construct.is_some() {
            self.start_section(token, false);
            if word == "as"
                && let Some(frame) = self.stack.last_mut()
            {
                frame.expect_name = true;
            }
            return;
        }
        self.leaf(token);
        // The pattern of a `match` arm starts after `case`: a reserved word
        // there is a recovered binder.
        if word == "case"
            && let Some(frame) = self.stack.last_mut()
            && frame.opener_word() == Some("match")
        {
            frame.expect_name = true;
        }
    }

    /// Whether the reserved word `word`, followed by `:`, is a name: a label,
    /// a binder, or a result name, recovered by the shaper. Directly in a
    /// bracket group every word before `:` is a label or a binder. Elsewhere
    /// an opener or a section keyword is, since none is ever followed by
    /// `:`; a closer there still closes its construct, as an `if` may be a
    /// dict key.
    fn names_before_colon(&self, word: &str) -> bool {
        let in_group = self
            .stack
            .last()
            .is_some_and(|frame| matches!(frame.kind, FrameKind::Group(_)));
        in_group || opener_keyword(word).is_some() || is_section(word)
    }

    /// Whether the innermost frame expects an expression next: its last
    /// significant token is the keyword of a section whose operand is an
    /// expression, the `else` of a guard, or a prefix word. A `do` there
    /// starts that expression, so it is a block, not a body.
    fn expects_operand(&self) -> bool {
        let Some(frame) = self.stack.last() else {
            return false;
        };
        let target = frame.section.as_ref().unwrap_or(&frame.items);
        target
            .iter()
            .rev()
            .find(|item| !is_trivia(item))
            .filter(|item| item.kind() == SyntaxKind::ReservedWord)
            .and_then(|item| item.span().text(self.text))
            .is_some_and(|word| EXPECTS_EXPRESSION.contains(&word))
    }

    fn start_section(&mut self, token: Token, body: bool) {
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        frame.close_section();
        frame.section = Some(vec![CstNode::leaf(SyntaxKind::ReservedWord, token.span)]);
        frame.body = body;
    }

    /// Resolves a closer by kind: `matches` selects the frames it can close.
    fn close_by_kind(&mut self, closer: Token, matches: impl Fn(FrameKind) -> bool) {
        let Some(position) = self.stack.iter().rposition(|frame| matches(frame.kind))
        else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxUnmatchedDelimiter,
                closer.span,
                "this closer closes nothing",
            ));
            let error = CstNode::interior(
                SyntaxKind::Error,
                vec![CstNode::leaf(
                    SyntaxKind::of_token(closer.kind),
                    closer.span,
                )],
                closer.span.start(),
            );
            self.target().push(error);
            return;
        };
        if position + 1 != self.stack.len()
            && let Some(innermost) = self.stack.last()
        {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::SyntaxUnmatchedDelimiter,
                    closer.span,
                    "this closer does not match the innermost open construct",
                )
                .with_related(innermost.opener, "the innermost open construct"),
            );
        }
        while self.stack.len() > position + 1 {
            self.close(None);
        }
        self.close(Some(closer));
    }

    /// Closes the innermost frame with its closer, or as missing.
    fn close(&mut self, closer: Option<Token>) {
        let Some(mut frame) = self.stack.pop() else {
            return;
        };
        frame.close_section();
        let mut trivia = Vec::new();
        match closer {
            Some(closer) => frame.items.push(CstNode::leaf(
                SyntaxKind::of_token(closer.kind),
                closer.span,
            )),
            None => {
                trivia = split_trailing_trivia(&mut frame.items);
                let at = frame
                    .items
                    .last()
                    .map_or(frame.opener.end(), |item| item.span().end());
                frame
                    .items
                    .push(CstNode::leaf(SyntaxKind::Missing, ByteSpan::empty_at(at)));
            }
        }
        let kind = match frame.kind {
            FrameKind::Group(opener) => group_kind(opener),
            FrameKind::Construct(opener) => construct_kind(opener, frame.top_level),
        };
        let node = CstNode::interior(kind, frame.items, frame.opener.start());
        let target = self.target();
        target.push(node);
        target.extend(trivia);
    }

    fn finish(mut self) -> Vec<CstNode> {
        while let Some(frame) = self.stack.last() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxUnmatchedDelimiter,
                frame.opener,
                "this construct is never closed",
            ));
            self.close(None);
        }
        self.top
    }
}

/// Builds the construct tree of `tokens`, the tokens of `text`, and returns
/// the items of the document's top level.
pub(crate) fn match_constructs(
    text: &str,
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<CstNode> {
    let mut matcher = Matcher {
        text,
        top: Vec::new(),
        stack: Vec::new(),
        diagnostics,
    };
    // Whether each token is followed, after trivia, by a `:`.
    let mut colon_after = vec![false; tokens.len()];
    let mut next_is_colon = false;
    for (index, token) in tokens.iter().enumerate().rev() {
        colon_after[index] = next_is_colon;
        if !token.kind.is_trivia() {
            next_is_colon = token.kind == TokenKind::Colon;
        }
    }
    for (token, colon) in tokens.iter().zip(colon_after) {
        matcher.token(*token, colon);
    }
    matcher.finish()
}
