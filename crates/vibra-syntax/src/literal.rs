//! Classification and decoding for the Step 5 literal surface.
//!
//! Literal values retain their original spelling while exposing decoded
//! character and string values. Numeric literals deliberately retain decimal
//! text instead of passing through a host integer or floating-point parser;
//! range checking and numeric typing belong to later milestones.

use vibra_diagnostics::DiagnosticCode;

/// The literal family responsible for a lexical diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvalidLiteralKind {
    /// A backslash character token is not a valid EDN character spelling.
    Character,
    /// A terminated quoted leaf contains an unsupported escape or scalar.
    String,
    /// A token beginning like a decimal number is malformed.
    Numeric,
}

impl InvalidLiteralKind {
    /// The diagnostic code for this invalid literal family.
    #[must_use]
    pub const fn diagnostic_code(self) -> DiagnosticCode {
        match self {
            Self::Character => DiagnosticCode::SyntaxInvalidCharacterLiteral,
            Self::String => DiagnosticCode::SyntaxInvalidStringLiteral,
            Self::Numeric => DiagnosticCode::SyntaxInvalidNumericLiteral,
        }
    }

    /// A stable human-facing message for this lexical failure.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Character => "invalid character literal",
            Self::String => "invalid string literal",
            Self::Numeric => "invalid numeric literal",
        }
    }
}

/// The result of classifying one non-trivia leaf token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiteralClassification {
    /// The leaf is a valid Step 5 literal.
    Literal(Literal),
    /// The leaf begins a literal family but violates its lexical production.
    Invalid(InvalidLiteralKind),
    /// The leaf is not a Step 5 literal; later steps may classify it as a name.
    Opaque,
}

impl LiteralClassification {
    /// The decoded literal, if classification succeeded.
    #[must_use]
    pub fn as_literal(&self) -> Option<&Literal> {
        match self {
            Self::Literal(literal) => Some(literal),
            Self::Invalid(_) | Self::Opaque => None,
        }
    }

    /// The invalid family, if this leaf is malformed literal syntax.
    #[must_use]
    pub const fn invalid_kind(&self) -> Option<InvalidLiteralKind> {
        match self {
            Self::Invalid(kind) => Some(*kind),
            Self::Literal(_) | Self::Opaque => None,
        }
    }
}

/// One decoded literal with its exact source spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// A double-quoted string.
    String(StringLiteral),
    /// An EDN-style character literal.
    Character(CharacterLiteral),
    /// A `true` or `false` literal.
    Boolean(BooleanLiteral),
    /// The single `void` literal.
    Void(VoidLiteral),
    /// A signed decimal integer with an optional exact suffix.
    Integer(IntegerLiteral),
    /// A decimal or suffixed floating-point literal.
    Float(FloatLiteral),
}

impl Literal {
    /// The literal's exact source spelling.
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            Self::String(literal) => literal.raw(),
            Self::Character(literal) => literal.raw(),
            Self::Boolean(literal) => literal.raw(),
            Self::Void(literal) => literal.raw(),
            Self::Integer(literal) => literal.raw(),
            Self::Float(literal) => literal.raw(),
        }
    }

    /// The broad kind of this literal.
    #[must_use]
    pub const fn kind(&self) -> LiteralKind {
        match self {
            Self::String(_) => LiteralKind::String,
            Self::Character(_) => LiteralKind::Character,
            Self::Boolean(_) => LiteralKind::Boolean,
            Self::Void(_) => LiteralKind::Void,
            Self::Integer(_) => LiteralKind::Integer,
            Self::Float(_) => LiteralKind::Float,
        }
    }
}

/// The broad category of a valid literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LiteralKind {
    /// A decoded string.
    String,
    /// A decoded Unicode scalar.
    Character,
    /// A decoded boolean.
    Boolean,
    /// The `void` value.
    Void,
    /// A decimal integer.
    Integer,
    /// A decimal floating-point spelling.
    Float,
}

/// A string literal and its decoded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringLiteral {
    raw: String,
    value: String,
}

impl StringLiteral {
    /// The exact quoted source spelling.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The value after decoding the supported escapes.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A character literal and its decoded Unicode scalar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterLiteral {
    raw: String,
    value: char,
}

impl CharacterLiteral {
    /// The exact backslash-prefixed source spelling.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The decoded Unicode scalar.
    #[must_use]
    pub const fn value(&self) -> char {
        self.value
    }
}

/// A boolean literal and its decoded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BooleanLiteral {
    raw: String,
    value: bool,
}

impl BooleanLiteral {
    /// The exact source spelling.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The decoded boolean.
    #[must_use]
    pub const fn value(&self) -> bool {
        self.value
    }
}

/// The exact suffix on an integer literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerSuffix {
    /// A signed eight-bit integer.
    I8,
    /// A signed sixteen-bit integer.
    I16,
    /// A signed thirty-two-bit integer.
    I32,
    /// A signed sixty-four-bit integer.
    I64,
    /// An unsigned eight-bit integer.
    U8,
    /// An unsigned sixteen-bit integer.
    U16,
    /// An unsigned thirty-two-bit integer.
    U32,
    /// An unsigned sixty-four-bit integer.
    U64,
}

impl IntegerSuffix {
    /// The canonical suffix spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            _ => None,
        }
    }
}

/// A signed decimal integer retained without host-width parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerLiteral {
    raw: String,
    negative: bool,
    digits: String,
    suffix: Option<IntegerSuffix>,
}

impl IntegerLiteral {
    /// The exact source spelling, including sign and suffix.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Whether the spelling has a leading minus sign.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    /// The decimal digits without sign or suffix.
    #[must_use]
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// The exact recognized integer suffix, if present.
    #[must_use]
    pub const fn suffix(&self) -> Option<IntegerSuffix> {
        self.suffix
    }
}

/// The exact suffix on a floating-point literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatSuffix {
    /// A thirty-two-bit floating-point literal.
    F32,
    /// A sixty-four-bit floating-point literal.
    F64,
}

impl FloatSuffix {
    /// The canonical suffix spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }
}

/// A decimal floating-point spelling retained without host rounding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloatLiteral {
    raw: String,
    body: String,
    suffix: Option<FloatSuffix>,
}

impl FloatLiteral {
    /// The exact source spelling, including suffix.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The decimal sign, digits, point, and exponent without the suffix.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The exact recognized float suffix, if present.
    #[must_use]
    pub const fn suffix(&self) -> Option<FloatSuffix> {
        self.suffix
    }
}

/// Classifies and, when valid, decodes a non-trivia leaf.
#[must_use]
pub fn classify(text: &str) -> LiteralClassification {
    if text.starts_with('"') {
        return classify_string(text);
    }
    if text.starts_with('\\') {
        return classify_character(text);
    }

    match text {
        "true" => LiteralClassification::Literal(Literal::Boolean(BooleanLiteral {
            raw: text.to_owned(),
            value: true,
        })),
        "false" => LiteralClassification::Literal(Literal::Boolean(BooleanLiteral {
            raw: text.to_owned(),
            value: false,
        })),
        "void" => LiteralClassification::Literal(Literal::Void(VoidLiteral {
            raw: text.to_owned(),
        })),
        _ if looks_like_numeric(text) => classify_numeric(text),
        _ => LiteralClassification::Opaque,
    }
}

/// Alias for [`classify`] when the caller is working specifically with
/// literal syntax.
#[must_use]
pub fn classify_literal(text: &str) -> LiteralClassification {
    classify(text)
}

/// Renders one decoded character using the canonical Step 5 spelling.
#[must_use]
pub fn canonical_character_spelling(value: char) -> String {
    match value {
        '\n' => "\\newline".to_owned(),
        '\r' => "\\return".to_owned(),
        ' ' => "\\space".to_owned(),
        '\t' => "\\tab".to_owned(),
        _ if value.is_control()
            || (value.is_whitespace() && (value as u32) <= 0xFFFF) =>
        {
            format!("\\u{:04X}", value as u32)
        }
        _ => {
            let mut spelling = String::from("\\");
            spelling.push(value);
            spelling
        }
    }
}

fn classify_string(text: &str) -> LiteralClassification {
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    else {
        return LiteralClassification::Invalid(InvalidLiteralKind::String);
    };

    let mut value = String::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            if character == '"' {
                return LiteralClassification::Invalid(InvalidLiteralKind::String);
            }
            value.push(character);
            continue;
        }

        let Some(escape) = characters.next() else {
            return LiteralClassification::Invalid(InvalidLiteralKind::String);
        };
        match escape {
            '"' => value.push('"'),
            '\\' => value.push('\\'),
            'n' => value.push('\n'),
            'r' => value.push('\r'),
            't' => value.push('\t'),
            'u' => {
                let Some(decoded) = decode_braced_unicode(&mut characters) else {
                    return LiteralClassification::Invalid(InvalidLiteralKind::String);
                };
                value.push(decoded);
            }
            _ => return LiteralClassification::Invalid(InvalidLiteralKind::String),
        }
    }

    LiteralClassification::Literal(Literal::String(StringLiteral {
        raw: text.to_owned(),
        value,
    }))
}

fn decode_braced_unicode(characters: &mut std::str::Chars<'_>) -> Option<char> {
    if characters.next() != Some('{') {
        return None;
    }

    let mut value = 0_u32;
    let mut digit_count = 0_usize;
    let mut closed = false;
    for character in characters.by_ref() {
        if character == '}' {
            closed = true;
            break;
        }
        let digit = hex_value(character)?;
        digit_count = digit_count.saturating_add(1);
        if digit_count > 6 {
            return None;
        }
        value = value.saturating_mul(16).saturating_add(digit);
    }

    if !closed || digit_count == 0 {
        return None;
    }
    char::from_u32(value)
}

fn classify_character(text: &str) -> LiteralClassification {
    let Some(body) = text.strip_prefix('\\') else {
        return LiteralClassification::Opaque;
    };

    let value = match body {
        "newline" => Some('\n'),
        "return" => Some('\r'),
        "space" => Some(' '),
        "tab" => Some('\t'),
        "u" => Some('u'),
        _ => body
            .strip_prefix('u')
            .and_then(decode_fixed_unicode)
            .or_else(|| {
                let mut characters = body.chars();
                let character = characters.next()?;
                (characters.next().is_none() && !character.is_whitespace())
                    .then_some(character)
            }),
    };

    match value {
        Some(value) => {
            LiteralClassification::Literal(Literal::Character(CharacterLiteral {
                raw: text.to_owned(),
                value,
            }))
        }
        None => LiteralClassification::Invalid(InvalidLiteralKind::Character),
    }
}

fn decode_fixed_unicode(text: &str) -> Option<char> {
    if text.len() != 4 {
        return None;
    }

    let mut value = 0_u32;
    for byte in text.bytes() {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'f' => u32::from(byte - b'a' + 10),
            b'A'..=b'F' => u32::from(byte - b'A' + 10),
            _ => return None,
        };
        value = value.saturating_mul(16).saturating_add(digit);
    }
    char::from_u32(value)
}

fn looks_like_numeric(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.first().is_some_and(|byte| byte.is_ascii_digit())
        || (bytes.first() == Some(&b'-')
            && bytes.get(1).is_some_and(|byte| byte.is_ascii_digit()))
}

fn classify_numeric(text: &str) -> LiteralClassification {
    let bytes = text.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let mut index = usize::from(negative);
    let digits_start = index;
    while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
        index = index.saturating_add(1);
    }
    let digits_end = index;
    let mut is_float = false;

    if bytes.get(index) == Some(&b'.') {
        is_float = true;
        index = index.saturating_add(1);
        let fraction_start = index;
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            index = index.saturating_add(1);
        }
        if fraction_start == index {
            return LiteralClassification::Invalid(InvalidLiteralKind::Numeric);
        }
    }

    if bytes
        .get(index)
        .is_some_and(|byte| matches!(*byte, b'e' | b'E'))
    {
        is_float = true;
        index = index.saturating_add(1);
        if bytes
            .get(index)
            .is_some_and(|byte| matches!(*byte, b'+' | b'-'))
        {
            index = index.saturating_add(1);
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            index = index.saturating_add(1);
        }
        if exponent_start == index {
            return LiteralClassification::Invalid(InvalidLiteralKind::Numeric);
        }
    }

    let suffix = text.get(index..).unwrap_or_default();
    if let Some(suffix_value) = IntegerSuffix::parse(suffix) {
        if is_float {
            return LiteralClassification::Invalid(InvalidLiteralKind::Numeric);
        }
        return LiteralClassification::Literal(Literal::Integer(IntegerLiteral {
            raw: text.to_owned(),
            negative,
            digits: text
                .get(digits_start..digits_end)
                .unwrap_or_default()
                .to_owned(),
            suffix: Some(suffix_value),
        }));
    }

    if let Some(suffix_value) = FloatSuffix::parse(suffix) {
        return LiteralClassification::Literal(Literal::Float(FloatLiteral {
            raw: text.to_owned(),
            body: text.get(..index).unwrap_or_default().to_owned(),
            suffix: Some(suffix_value),
        }));
    }

    if !suffix.is_empty() {
        return LiteralClassification::Invalid(InvalidLiteralKind::Numeric);
    }

    if is_float {
        LiteralClassification::Literal(Literal::Float(FloatLiteral {
            raw: text.to_owned(),
            body: text.get(..index).unwrap_or_default().to_owned(),
            suffix: None,
        }))
    } else {
        LiteralClassification::Literal(Literal::Integer(IntegerLiteral {
            raw: text.to_owned(),
            negative,
            digits: text
                .get(digits_start..digits_end)
                .unwrap_or_default()
                .to_owned(),
            suffix: None,
        }))
    }
}

fn hex_value(character: char) -> Option<u32> {
    match character {
        '0'..='9' => Some(u32::from(character as u8 - b'0')),
        'a'..='f' => Some(u32::from(character as u8 - b'a' + 10)),
        'A'..='F' => Some(u32::from(character as u8 - b'A' + 10)),
        _ => None,
    }
}

/// The `void` literal and its exact source spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoidLiteral {
    raw: String,
}

impl VoidLiteral {
    /// The exact source spelling, currently always `void`.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}
