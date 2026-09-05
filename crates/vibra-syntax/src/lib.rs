//! Vibra's reader: lexer, lossless recovery CST, document modes, and AST.
//!
//! One lexer serves both document grammars. The `.vib` source grammar and the
//! `.vibon` data grammar are selected by extension before parsing and are
//! never inferred from contents. `docs/spec/01-source-language.md` and the
//! VIBON section of `docs/spec/04-programs-and-packages.md` are the normative
//! definitions.
//!
//! # Position in the architecture
//!
//! This is the first node of the roadmap's dependency chain. It may depend on
//! [`vibra_diagnostics`] and nothing else in the workspace. It must never
//! depend on the formatter, the schemas, or any tool surface.
//!
//! # Status
//!
//! Milestone 1 steps 4–9 supply the shared UTF-8 lexer, literal/name
//! classification, contextual declaration/type/expression/pattern views,
//! generic VIBON data decoding, hand-rolled lossless recovery CST, and
//! extension-selected document modes. Project schemas, resolution, and
//! semantic checking remain later-step concerns; see
//! `docs/roadmap/milestone-1/README.md`.

mod ast;
mod data;
mod literal;
mod name;
mod reader;

pub use ast::{
    Application, ApplicationBinding, Attribute, BindingError, BindingFacts,
    CallArgument, Declaration, DeclarationAttributes, DefDeclaration,
    DeffectDeclaration, DefintDeclaration, DeftypeBody, DeftypeDeclaration, EffectRow,
    Expression, ExpressionKind, FunctionAttributes, FunctionDeclaration, FunctionType,
    GenericBinding, ImplDeclaration, ImportDeclaration, LabelledParameter,
    LambdaExpression, MatchArm, Parameter, Pattern, PatternArgument, PatternKind,
    RawNode, SourceAst, SourceDecode, TestDeclaration, TypeAttributes, TypeExpr,
    TypeField, TypeMember, TypeSlot, VariadicBinding, VariadicParameter, VariadicType,
    contains_declaration_head, decode_source_root,
};
pub use data::{
    AtomRole, DataDecode, DataField, DataNode, DataValue, TypedDataSchema,
    canonical_data, decode_data_root,
};
pub use literal::{
    BooleanLiteral, CharacterLiteral, FloatLiteral, FloatSuffix, IntegerLiteral,
    IntegerSuffix, InvalidLiteralKind, Literal, LiteralClassification, LiteralKind,
    StringLiteral, VoidLiteral, canonical_character_spelling, classify,
    classify_literal,
};
pub use name::{Name, NameClassification, NameKind, classify_name};
pub use reader::{
    CstNode, Document, DocumentMode, DocumentModeError, Lexed, Lexer, SyntaxKind,
    Token, TokenKind, lex, lex_bytes, parse, parse_data, parse_document, parse_source,
    parse_with_mode,
};
