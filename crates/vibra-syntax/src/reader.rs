//! The Step 4–10 UTF-8 reader spine, literal/name surface, contextual
//! declaration/type/expression/pattern view, and structural query surface.
//!
//! This module keeps one lossless, delimiter-aware tree. Step 5 literal, Step 6
//! name, and Step 8–9 contextual classification are layered over retained leaf
//! text without changing source bytes. Resolution and semantic checking remain
//! later-step concerns. Step 10 queries remain structural: they do not resolve
//! names or infer types.

use std::fmt;
use std::path::{Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};

use crate::ast::{SourceAst, decode_source_root};
use crate::data::{DataNode, decode_data_root};
use crate::literal::{LiteralClassification, classify};
use crate::name::{NameClassification, classify_name};

/// The grammar selected for a document by its filename extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocumentMode {
    /// Executable Vibra source (`.vib`).
    Source,
    /// Non-executable Vibra Object Notation (`.vibon`).
    Data,
}

impl DocumentMode {
    /// Selects a document mode from an exact filename extension.
    ///
    /// Selection is deliberately performed before lexing. There is no content
    /// sniffing, extension fallback, or legacy extension alias.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, DocumentModeError> {
        let path = path.as_ref();
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("vib") => Ok(Self::Source),
            Some("vibon") => Ok(Self::Data),
            _ => Err(DocumentModeError::UnsupportedExtension {
                path: path.to_path_buf(),
            }),
        }
    }

    /// Alias for [`Self::from_path`] for callers that describe mode selection
    /// as a path lookup.
    pub fn for_path(path: impl AsRef<Path>) -> Result<Self, DocumentModeError> {
        Self::from_path(path)
    }

    /// The canonical extension for this mode.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Source => "vib",
            Self::Data => "vibon",
        }
    }

    /// The loader-facing name used in diagnostics and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Data => "data",
        }
    }
}

/// A path that cannot select one of the two v1 document modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentModeError {
    /// The path has no exact `.vib` or `.vibon` extension.
    UnsupportedExtension {
        /// The path supplied by the caller.
        path: PathBuf,
    },
}

impl DocumentModeError {
    /// The path that failed mode selection.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::UnsupportedExtension { path } => path,
        }
    }
}

impl fmt::Display for DocumentModeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedExtension { path } => write!(
                formatter,
                "unsupported document extension for {} (expected .vib or .vibon)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DocumentModeError {}

/// A token kind emitted by the lossless UTF-8 lexer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenKind {
    /// A run of Unicode whitespace, including line terminators.
    Whitespace,
    /// A semicolon through the end of its line, excluding the line terminator.
    LineComment,
    /// An opening list delimiter.
    OpenParen,
    /// A closing list delimiter.
    CloseParen,
    /// A non-delimiter, non-trivia leaf token. Its text is intentionally opaque.
    Atom,
    /// The zero-width end-of-file marker.
    Eof,
}

/// One lossless token and its source byte span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    span: ByteSpan,
    text: String,
    unterminated_quote: bool,
}

impl Token {
    fn new(kind: TokenKind, span: ByteSpan, text: &str) -> Self {
        Self {
            kind,
            span,
            text: text.to_owned(),
            unterminated_quote: false,
        }
    }

    /// The token category.
    #[must_use]
    pub const fn kind(&self) -> TokenKind {
        self.kind
    }

    /// The half-open UTF-8 byte span in the source document.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The exact token text. EOF has an empty text string.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Classifies this atom as a Step 5 literal without losing its raw text.
    ///
    /// Trivia, delimiters, EOF, and an unterminated quoted leaf do not expose
    /// a literal classification because they are not complete leaves.
    #[must_use]
    pub fn literal(&self) -> Option<LiteralClassification> {
        if self.kind != TokenKind::Atom || self.unterminated_quote {
            return None;
        }
        Some(classify(&self.text))
    }

    /// Classifies this complete nonliteral atom as a Step 6 name.
    #[must_use]
    pub fn name(&self) -> Option<NameClassification> {
        if self.kind != TokenKind::Atom || self.unterminated_quote {
            return None;
        }
        match classify(&self.text) {
            LiteralClassification::Literal(_) | LiteralClassification::Invalid(_) => {
                None
            }
            LiteralClassification::Opaque => Some(classify_name(&self.text)),
        }
    }

    fn is_unterminated_quote(&self) -> bool {
        self.unterminated_quote
    }

    fn as_node(&self, kind: SyntaxKind) -> CstNode {
        CstNode::leaf_with_quote_state(
            kind,
            self.span,
            &self.text,
            self.unterminated_quote,
        )
    }
}

/// The result of lexing one UTF-8 document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexed {
    source: String,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexed {
    /// The input text retained for lossless consumers.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Tokens in source order, including a zero-width EOF token.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Consumes the lexed result and returns its tokens, including EOF.
    #[must_use]
    pub fn into_tokens(self) -> Vec<Token> {
        self.tokens
    }

    /// Lexer diagnostics in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether lexing emitted no diagnostics.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// A UTF-8 lexer for the shared source/data reader.
#[derive(Clone, Copy, Debug)]
pub struct Lexer<'source> {
    source: &'source str,
}

impl<'source> Lexer<'source> {
    /// Creates a lexer over valid UTF-8 text.
    #[must_use]
    pub const fn new(source: &'source str) -> Self {
        Self { source }
    }

    /// Lexes the input while retaining every byte in token text.
    #[must_use]
    pub fn lex(self) -> Lexed {
        lex(self.source)
    }
}

/// Lexes valid UTF-8 text into lossless tokens.
#[must_use]
pub fn lex(source: &str) -> Lexed {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut offset = 0;

    while offset < source.len() {
        let Some(character) = source[offset..].chars().next() else {
            break;
        };
        let start = offset;

        if character.is_whitespace() {
            offset = advance_one(source, offset);
            while offset < source.len() {
                let Some(next) = source[offset..].chars().next() else {
                    break;
                };
                if !next.is_whitespace() {
                    break;
                }
                offset = advance_one(source, offset);
            }
            tokens.push(Token::new(
                TokenKind::Whitespace,
                ByteSpan::new(start, offset),
                &source[start..offset],
            ));
            continue;
        }

        if character == ';' {
            offset = advance_one(source, offset);
            while offset < source.len() {
                let Some(next) = source[offset..].chars().next() else {
                    break;
                };
                if next == '\n' || next == '\r' {
                    break;
                }
                offset = advance_one(source, offset);
            }
            tokens.push(Token::new(
                TokenKind::LineComment,
                ByteSpan::new(start, offset),
                &source[start..offset],
            ));
            continue;
        }

        let mut unterminated_quote = false;
        let kind = match character {
            '(' => {
                offset = advance_one(source, offset);
                TokenKind::OpenParen
            }
            ')' => {
                offset = advance_one(source, offset);
                TokenKind::CloseParen
            }
            '"' => {
                // A quoted leaf is kept together so whitespace, semicolons,
                // and delimiters inside it remain lossless. Escape validity is
                // a later literal-surface concern.
                offset = advance_one(source, offset);
                let mut escaped = false;
                let mut closed = false;
                while offset < source.len() {
                    let Some(next) = source[offset..].chars().next() else {
                        break;
                    };
                    offset = advance_one(source, offset);
                    if escaped {
                        escaped = false;
                    } else if next == '\\' {
                        escaped = true;
                    } else if next == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    unterminated_quote = true;
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxUnmatchedDelimiter,
                        ByteSpan::empty_at(source.len()),
                        "quoted leaf is not closed before the end of the document",
                    ));
                }
                TokenKind::Atom
            }
            '\\' => {
                // A character literal owns a direct delimiter or quote after
                // its backslash. This branch must run before generic
                // delimiter/comment handling so `\)`, `\;`, and `\"` stay
                // one lossless token.
                offset = scan_character_token(source, start);
                TokenKind::Atom
            }
            _ => {
                offset = advance_one(source, offset);
                while offset < source.len() {
                    let Some(next) = source[offset..].chars().next() else {
                        break;
                    };
                    if next.is_whitespace() || matches!(next, ';' | '(' | ')') {
                        break;
                    }
                    offset = advance_one(source, offset);
                }
                TokenKind::Atom
            }
        };
        let mut token =
            Token::new(kind, ByteSpan::new(start, offset), &source[start..offset]);
        token.unterminated_quote = unterminated_quote;
        if !unterminated_quote && token.kind() == TokenKind::Atom {
            match classify(token.text()) {
                LiteralClassification::Invalid(kind) => {
                    diagnostics.push(Diagnostic::new(
                        kind.diagnostic_code(),
                        token.span(),
                        kind.message(),
                    ));
                }
                LiteralClassification::Opaque
                    if matches!(
                        classify_name(token.text()),
                        NameClassification::Invalid
                    ) =>
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidName,
                        token.span(),
                        "invalid name spelling",
                    ));
                }
                LiteralClassification::Literal(_) | LiteralClassification::Opaque => {}
            }
        }
        tokens.push(token);
    }

    tokens.push(Token::new(
        TokenKind::Eof,
        ByteSpan::empty_at(source.len()),
        "",
    ));

    Lexed {
        source: source.to_owned(),
        tokens,
        diagnostics,
    }
}

fn advance_one(source: &str, offset: usize) -> usize {
    source[offset..]
        .chars()
        .next()
        .map_or(source.len(), |character| offset + character.len_utf8())
}

fn scan_character_token(source: &str, start: usize) -> usize {
    let mut offset = start.saturating_add(1);
    let Some(character) = source.get(offset..).and_then(|text| text.chars().next())
    else {
        return offset.min(source.len());
    };

    if character.is_whitespace() {
        return offset;
    }
    if matches!(character, ';' | '(' | ')' | '"') {
        return advance_one(source, offset);
    }

    offset = advance_one(source, offset);
    while offset < source.len() {
        let Some(next) = source[offset..].chars().next() else {
            break;
        };
        if next.is_whitespace() || matches!(next, ';' | '(' | ')' | '"') {
            break;
        }
        offset = advance_one(source, offset);
    }
    offset
}

/// The kinds of nodes in the hand-rolled lossless recovery tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxKind {
    /// The complete document.
    Root,
    /// A parenthesized list, possibly missing its closing delimiter.
    List,
    /// An opaque leaf token.
    Atom,
    /// A run of whitespace trivia.
    Whitespace,
    /// A semicolon line comment.
    LineComment,
    /// The opening delimiter token inside a list.
    OpenParen,
    /// The closing delimiter token inside a list.
    CloseParen,
    /// A recovered unmatched delimiter or missing-close marker.
    Error,
}

/// One node in the lossless, recovery-oriented concrete syntax tree.
pub struct CstNode {
    kind: SyntaxKind,
    span: ByteSpan,
    text: String,
    children: Vec<CstNode>,
    unterminated_quote: bool,
}

impl fmt::Debug for CstNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CstNode")
            .field("kind", &self.kind)
            .field("span", &self.span)
            .field("leaf_text", &self.leaf_text())
            .field("child_count", &self.children.len())
            .finish()
    }
}

impl Drop for CstNode {
    fn drop(&mut self) {
        // A deeply nested CST must not recurse through Rust's default field
        // drop glue. Drain descendants iteratively and leave each popped
        // node with an empty child vector before it is dropped.
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut node) = pending.pop() {
            pending.append(&mut node.children);
        }
    }
}

impl CstNode {
    fn leaf(kind: SyntaxKind, span: ByteSpan, text: &str) -> Self {
        Self::leaf_with_quote_state(kind, span, text, false)
    }

    fn leaf_with_quote_state(
        kind: SyntaxKind,
        span: ByteSpan,
        text: &str,
        unterminated_quote: bool,
    ) -> Self {
        Self {
            kind,
            span,
            text: text.to_owned(),
            children: Vec::new(),
            unterminated_quote,
        }
    }

    fn composite(kind: SyntaxKind, span: ByteSpan, children: Vec<CstNode>) -> Self {
        Self {
            kind,
            span,
            // Composite text is reconstructed on demand. Caching recursively
            // here made construction quadratic for deeply nested input.
            text: String::new(),
            children,
            unterminated_quote: false,
        }
    }

    /// The structural kind of this node.
    #[must_use]
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// The half-open source byte span covered by this node.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Exact token text for a leaf node.
    ///
    /// Composite nodes return `None`; use [`Self::to_source`] to reconstruct
    /// the exact text covered by an arbitrary subtree without recursive
    /// caching.
    #[must_use]
    pub fn leaf_text(&self) -> Option<&str> {
        (!matches!(self.kind, SyntaxKind::Root | SyntaxKind::List))
            .then_some(&self.text)
    }

    /// Classifies an atom leaf as a Step 5 literal without changing its raw
    /// source spelling.
    #[must_use]
    pub fn literal(&self) -> Option<LiteralClassification> {
        (self.kind == SyntaxKind::Atom && !self.unterminated_quote)
            .then(|| classify(&self.text))
    }

    /// Classifies a complete nonliteral atom as a Step 6 name.
    #[must_use]
    pub fn name(&self) -> Option<NameClassification> {
        if self.kind != SyntaxKind::Atom || self.unterminated_quote {
            return None;
        }
        match classify(&self.text) {
            LiteralClassification::Literal(_) | LiteralClassification::Invalid(_) => {
                None
            }
            LiteralClassification::Opaque => Some(classify_name(&self.text)),
        }
    }

    /// Child nodes in source order. Trivia and delimiters are retained.
    #[must_use]
    pub fn children(&self) -> &[CstNode] {
        &self.children
    }

    /// Reconstructs the exact source represented by this subtree.
    #[must_use]
    pub fn to_source(&self) -> String {
        if self.children.is_empty() {
            return self.text.clone();
        }
        let mut output = String::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.children.is_empty() {
                output.push_str(&node.text);
            } else {
                stack.extend(node.children.iter().rev());
            }
        }
        output
    }

    /// Whether this subtree contains a recovery error marker.
    #[must_use]
    pub fn contains_error(&self) -> bool {
        self.contains_kind(SyntaxKind::Error)
    }

    /// Whether this subtree contains a node of `kind`.
    #[must_use]
    pub fn contains_kind(&self, kind: SyntaxKind) -> bool {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.kind == kind {
                return true;
            }
            stack.extend(node.children.iter());
        }
        false
    }
}

/// A parsed document with an extension-selected mode and a lossless CST.
pub struct Document {
    path: PathBuf,
    mode: DocumentMode,
    source: String,
    lexed: Lexed,
    root: CstNode,
    diagnostics: Vec<Diagnostic>,
    data: Option<DataNode>,
    ast: Option<SourceAst>,
}

impl fmt::Debug for Document {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Document")
            .field("path", &self.path)
            .field("mode", &self.mode)
            .field("source_len", &self.source.len())
            .field("token_count", &self.lexed.tokens().len())
            .field("root", &self.root)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl Document {
    /// Parses a document using its exact filename extension.
    pub fn parse(
        path: impl AsRef<Path>,
        source: &str,
    ) -> Result<Self, DocumentModeError> {
        parse_document(path, source)
    }

    /// Parses a document through an explicitly selected loader mode.
    pub fn parse_with_mode(
        path: impl AsRef<Path>,
        source: &str,
        expected_mode: DocumentMode,
    ) -> Result<Self, DocumentModeError> {
        parse_with_mode(path, source, expected_mode)
    }

    /// The path used for extension selection.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The selected source or data grammar.
    #[must_use]
    pub const fn mode(&self) -> DocumentMode {
        self.mode
    }

    /// The original UTF-8 source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The lossless token stream, including EOF.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        self.lexed.tokens()
    }

    /// The document root node.
    #[must_use]
    pub const fn root(&self) -> &CstNode {
        &self.root
    }

    /// Diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the document has no error-level syntax or loader diagnostics.
    #[must_use]
    pub fn accepted(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == Level::Error)
    }

    /// Whether recovery inserted an error marker into the tree.
    #[must_use]
    pub fn recovered(&self) -> bool {
        self.root.contains_error()
    }

    /// The decoded generic VIBON root, when this is a valid data document.
    #[must_use]
    pub const fn data(&self) -> Option<&DataNode> {
        self.data.as_ref()
    }

    /// The native declaration/type AST for a recognized source module.
    #[must_use]
    pub const fn ast(&self) -> Option<&SourceAst> {
        self.ast.as_ref()
    }

    /// Returns structural facts for the smallest CST node at a UTF-8 byte
    /// offset.
    pub fn query_position(
        &self,
        offset: usize,
    ) -> Result<crate::query::StructuralQuery, crate::query::QueryError> {
        crate::query::query_position(self, offset)
    }
}

/// Parses a document using the exact mode selected by its extension.
pub fn parse_document(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<Document, DocumentModeError> {
    let path = path.as_ref();
    let mode = DocumentMode::from_path(path)?;
    Ok(parse_selected(path, mode, source))
}

/// Short alias for [`parse_document`].
pub fn parse(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<Document, DocumentModeError> {
    parse_document(path, source)
}

/// Lexes bytes after validating that they are UTF-8.
pub fn lex_bytes(bytes: &[u8]) -> Result<Lexed, std::str::Utf8Error> {
    std::str::from_utf8(bytes).map(lex)
}

/// Parses with an explicitly requested loader mode.
///
/// If the extension selects the other grammar, the function returns a
/// document containing only `@data.invalid-extension`. Contents are not
/// lexed or sniffed in that case.
pub fn parse_with_mode(
    path: impl AsRef<Path>,
    source: &str,
    expected_mode: DocumentMode,
) -> Result<Document, DocumentModeError> {
    let path = path.as_ref();
    let selected_mode = DocumentMode::from_path(path)?;
    if selected_mode != expected_mode {
        return Ok(invalid_extension_document(
            path,
            expected_mode,
            selected_mode,
        ));
    }
    Ok(parse_selected(path, selected_mode, source))
}

/// Parses through the source loader, requiring a `.vib` extension.
pub fn parse_source(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<Document, DocumentModeError> {
    parse_with_mode(path, source, DocumentMode::Source)
}

/// Parses through the persistent-data loader, requiring a `.vibon` extension.
pub fn parse_data(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<Document, DocumentModeError> {
    parse_with_mode(path, source, DocumentMode::Data)
}

fn parse_selected(path: &Path, mode: DocumentMode, source: &str) -> Document {
    let lexed = lex(source);
    let tokens = lexed.tokens.clone();
    let mut parser = Parser::new(&tokens, source, lexed.diagnostics.clone());
    let root = parser.parse_root();
    let source_decode =
        (mode == DocumentMode::Source).then(|| decode_source_root(&root));
    let ast = source_decode
        .as_ref()
        .and_then(|decoded| decoded.ast().cloned());
    let data_decode = (mode == DocumentMode::Data).then(|| decode_data_root(&root));
    let data = data_decode
        .as_ref()
        .and_then(|decoded| decoded.value().cloned());
    if let Some(decoded) = &data_decode {
        parser.diagnostics.extend_from_slice(decoded.diagnostics());
    }
    if let Some(decoded) = &source_decode
        && decoded.recognized()
    {
        parser.diagnostics.extend_from_slice(decoded.diagnostics());
    }
    // Lexer errors are collected before parsing, but consumers see one
    // source-ordered stream. Stable sorting preserves emission order at EOF.
    parser.diagnostics.sort_by_key(|diagnostic| {
        let span = diagnostic.primary_span();
        (span.start(), span.end())
    });
    Document {
        path: path.to_path_buf(),
        mode,
        source: source.to_owned(),
        lexed,
        root,
        diagnostics: parser.diagnostics,
        data,
        ast,
    }
}

fn invalid_extension_document(
    path: &Path,
    expected: DocumentMode,
    selected: DocumentMode,
) -> Document {
    let source = String::new();
    let lexed = lex(&source);
    let root = CstNode::composite(SyntaxKind::Root, ByteSpan::empty_at(0), Vec::new());
    let diagnostic = Diagnostic::new(
        DiagnosticCode::DataInvalidExtension,
        ByteSpan::empty_at(0),
        format!(
            "{} loader cannot read a .{} document selected as {}",
            expected.as_str(),
            selected.extension(),
            selected.as_str()
        ),
    );
    Document {
        path: path.to_path_buf(),
        mode: expected,
        source,
        lexed,
        root,
        diagnostics: vec![diagnostic],
        data: None,
        ast: None,
    }
}

struct Parser<'source> {
    tokens: &'source [Token],
    source: &'source str,
    index: usize,
    diagnostics: Vec<Diagnostic>,
}

struct ListFrame {
    start: usize,
    children: Vec<CstNode>,
    previous_form: bool,
}

impl<'source> Parser<'source> {
    fn new(
        tokens: &'source [Token],
        source: &'source str,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            tokens,
            source,
            index: 0,
            diagnostics,
        }
    }

    fn parse_root(&mut self) -> CstNode {
        let mut children = Vec::new();
        let mut open_lists = Vec::new();

        loop {
            let Some(token) = self.current().cloned() else {
                break;
            };
            match token.kind() {
                TokenKind::Eof => {
                    let eof = token.span().start();
                    self.close_open_lists(&mut open_lists, &mut children, eof);
                    break;
                }
                TokenKind::Whitespace => {
                    if let Some(list) = open_lists.last_mut() {
                        list.children.push(token.as_node(SyntaxKind::Whitespace));
                        list.previous_form = false;
                    } else {
                        children.push(token.as_node(SyntaxKind::Whitespace));
                    }
                    self.advance();
                }
                TokenKind::LineComment => {
                    if let Some(list) = open_lists.last_mut() {
                        list.children.push(token.as_node(SyntaxKind::LineComment));
                        list.previous_form = false;
                    } else {
                        children.push(token.as_node(SyntaxKind::LineComment));
                    }
                    self.advance();
                }
                TokenKind::OpenParen => {
                    let needs_separator =
                        open_lists.last().is_some_and(|list| list.previous_form);
                    if needs_separator {
                        self.missing_separator(token.span().start());
                        if let Some(list) = open_lists.last_mut() {
                            list.children.push(CstNode::leaf(
                                SyntaxKind::Error,
                                ByteSpan::empty_at(token.span().start()),
                                "",
                            ));
                        }
                    }
                    open_lists.push(ListFrame {
                        start: token.span().start(),
                        children: vec![token.as_node(SyntaxKind::OpenParen)],
                        previous_form: false,
                    });
                    self.advance();
                }
                TokenKind::CloseParen => {
                    let Some(mut list) = open_lists.pop() else {
                        self.unmatched_close(&token);
                        children.push(token.as_node(SyntaxKind::Error));
                        self.advance();
                        continue;
                    };
                    let end = token.span().end();
                    list.children.push(token.as_node(SyntaxKind::CloseParen));
                    let node = CstNode::composite(
                        SyntaxKind::List,
                        ByteSpan::new(list.start, end),
                        list.children,
                    );
                    self.attach_completed_list(&mut open_lists, &mut children, node);
                    self.advance();
                }
                TokenKind::Atom => {
                    let needs_separator =
                        open_lists.last().is_some_and(|list| list.previous_form);
                    if needs_separator {
                        self.missing_separator(token.span().start());
                        if let Some(list) = open_lists.last_mut() {
                            list.children.push(CstNode::leaf(
                                SyntaxKind::Error,
                                ByteSpan::empty_at(token.span().start()),
                                "",
                            ));
                        }
                    }

                    let node = token.as_node(SyntaxKind::Atom);
                    if let Some(list) = open_lists.last_mut() {
                        list.children.push(node);
                        if token.is_unterminated_quote() {
                            list.children.push(CstNode::leaf(
                                SyntaxKind::Error,
                                ByteSpan::empty_at(token.span().end()),
                                "",
                            ));
                        }
                        list.previous_form = true;
                    } else {
                        children.push(node);
                        if token.is_unterminated_quote() {
                            children.push(CstNode::leaf(
                                SyntaxKind::Error,
                                ByteSpan::empty_at(token.span().end()),
                                "",
                            ));
                        }
                    }
                    self.advance();
                }
            }
        }

        CstNode::composite(
            SyntaxKind::Root,
            ByteSpan::new(0, self.source.len()),
            children,
        )
    }

    fn close_open_lists(
        &mut self,
        open_lists: &mut Vec<ListFrame>,
        root_children: &mut Vec<CstNode>,
        eof: usize,
    ) {
        while let Some(mut list) = open_lists.pop() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxUnmatchedDelimiter,
                ByteSpan::empty_at(eof),
                "list is not closed before the end of the document",
            ));
            list.children.push(CstNode::leaf(
                SyntaxKind::Error,
                ByteSpan::empty_at(eof),
                "",
            ));
            let node = CstNode::composite(
                SyntaxKind::List,
                ByteSpan::new(list.start, eof),
                list.children,
            );
            self.attach_completed_list(open_lists, root_children, node);
        }
    }

    fn attach_completed_list(
        &mut self,
        open_lists: &mut [ListFrame],
        root_children: &mut Vec<CstNode>,
        node: CstNode,
    ) {
        if let Some(parent) = open_lists.last_mut() {
            parent.children.push(node);
            parent.previous_form = true;
        } else {
            root_children.push(node);
        }
    }

    fn unmatched_close(&mut self, token: &Token) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxUnmatchedDelimiter,
            token.span(),
            "closing delimiter has no matching opening delimiter",
        ));
    }

    fn missing_separator(&mut self, offset: usize) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxMissingSeparator,
            ByteSpan::empty_at(offset),
            "sibling forms in a list must be separated by trivia",
        ));
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn advance(&mut self) {
        self.index = self.index.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{CstNode, Document, DocumentMode, SyntaxKind, lex};
    use std::path::PathBuf;
    use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

    #[test]
    fn warning_only_document_remains_accepted() {
        let source = String::new();
        let document = Document {
            path: PathBuf::from("warning.vib"),
            mode: DocumentMode::Source,
            source,
            lexed: lex(""),
            root: CstNode::composite(
                SyntaxKind::Root,
                ByteSpan::empty_at(0),
                Vec::new(),
            ),
            diagnostics: vec![Diagnostic::new(
                DiagnosticCode::StyleArgumentOrder,
                ByteSpan::empty_at(0),
                "operands are in a noncanonical order",
            )],
            data: None,
            ast: None,
        };

        assert!(document.accepted());
    }
}
