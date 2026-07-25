use super::{Span, SyntaxError};

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    OpenParen,
    CloseParen,
    Symbol(String),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
    Comment(String),
}

pub fn lex(source: &str) -> Result<Vec<Token>, SyntaxError> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn lex(mut self) -> Result<Vec<Token>, SyntaxError> {
        let mut tokens = Vec::new();
        while self.offset < self.source.len() {
            let ch = self.peek();
            if ch.is_whitespace() {
                self.bump();
                continue;
            }
            let start = self.offset;
            let kind = match ch {
                '(' => {
                    self.bump();
                    TokenKind::OpenParen
                }
                ')' => {
                    self.bump();
                    TokenKind::CloseParen
                }
                ';' => self.comment(),
                '"' => self.string()?,
                '[' | ']' | '{' | '}' | ',' => {
                    self.bump();
                    return Err(SyntaxError::new(
                        "E-SYN-001",
                        format!("`{ch}` is not part of Vibra S-expression syntax"),
                        Span::new(start, self.offset),
                    ));
                }
                _ if ch.is_control() => {
                    self.bump();
                    return Err(SyntaxError::new(
                        "E-SYN-001",
                        "control character is not valid outside a string",
                        Span::new(start, self.offset),
                    ));
                }
                _ => self.atom()?,
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, self.offset),
            });
        }
        Ok(tokens)
    }

    fn comment(&mut self) -> TokenKind {
        self.bump();
        let start = self.offset;
        while self.offset < self.source.len() && self.peek() != '\n' {
            self.bump();
        }
        TokenKind::Comment(self.source[start..self.offset].to_string())
    }

    fn string(&mut self) -> Result<TokenKind, SyntaxError> {
        let start = self.offset;
        self.bump();
        let mut value = String::new();
        while self.offset < self.source.len() {
            let ch = self.bump();
            match ch {
                '"' => return Ok(TokenKind::String(value)),
                '\\' => value.push(self.escape(start)?),
                '\n' | '\r' => {
                    return Err(SyntaxError::new(
                        "E-SYN-002",
                        "string literal cannot contain an unescaped newline",
                        Span::new(start, self.offset),
                    ));
                }
                other => value.push(other),
            }
        }
        Err(SyntaxError::new(
            "E-SYN-002",
            "unterminated string literal",
            Span::new(start, self.offset),
        ))
    }

    fn escape(&mut self, string_start: usize) -> Result<char, SyntaxError> {
        let escape_start = self.offset.saturating_sub(1);
        if self.offset == self.source.len() {
            return Err(SyntaxError::new(
                "E-SYN-002",
                "unterminated string escape",
                Span::new(string_start, self.offset),
            ));
        }
        Ok(match self.bump() {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => return self.unicode_escape(escape_start),
            other => {
                return Err(SyntaxError::new(
                    "E-SYN-003",
                    format!("unknown string escape `\\{other}`"),
                    Span::new(escape_start, self.offset),
                ));
            }
        })
    }

    fn unicode_escape(&mut self, escape_start: usize) -> Result<char, SyntaxError> {
        if self.offset == self.source.len() || self.bump() != '{' {
            return Err(SyntaxError::new(
                "E-SYN-003",
                "Unicode escapes use `\\u{HEX}`",
                Span::new(escape_start, self.offset),
            ));
        }
        let digits_start = self.offset;
        while self.offset < self.source.len() && self.peek().is_ascii_hexdigit() {
            self.bump();
        }
        let digits = &self.source[digits_start..self.offset];
        if digits.is_empty()
            || digits.len() > 6
            || self.offset == self.source.len()
            || self.bump() != '}'
        {
            return Err(SyntaxError::new(
                "E-SYN-003",
                "invalid Unicode escape; expected one to six hexadecimal digits",
                Span::new(escape_start, self.offset),
            ));
        }
        let scalar = u32::from_str_radix(digits, 16).expect("digits were checked");
        char::from_u32(scalar).ok_or_else(|| {
            SyntaxError::new(
                "E-SYN-003",
                "Unicode escape is not a scalar value",
                Span::new(escape_start, self.offset),
            )
        })
    }

    fn atom(&mut self) -> Result<TokenKind, SyntaxError> {
        let start = self.offset;
        while self.offset < self.source.len() && !is_delimiter(self.peek()) {
            let ch = self.peek();
            if matches!(ch, '[' | ']' | '{' | '}' | ',' | '"') || ch.is_control() {
                self.bump();
                return Err(SyntaxError::new(
                    "E-SYN-001",
                    format!("invalid character `{ch}` in token"),
                    Span::new(start, self.offset),
                ));
            }
            self.bump();
        }
        let text = &self.source[start..self.offset];
        if text.is_empty() {
            return Err(SyntaxError::new(
                "E-SYN-001",
                "expected a token",
                Span::new(start, self.offset),
            ));
        }
        match text {
            "true" => Ok(TokenKind::Bool(true)),
            "false" => Ok(TokenKind::Bool(false)),
            "unit" => Ok(TokenKind::Unit),
            _ if looks_numeric(text) => parse_number(text, Span::new(start, self.offset)),
            _ if is_symbol(text) => Ok(TokenKind::Symbol(text.to_string())),
            _ => Err(SyntaxError::new(
                "E-SYN-001",
                format!("invalid symbol `{text}`"),
                Span::new(start, self.offset),
            )),
        }
    }

    fn peek(&self) -> char {
        self.source[self.offset..]
            .chars()
            .next()
            .expect("offset is in bounds")
    }

    fn bump(&mut self) -> char {
        let ch = self.peek();
        self.offset += ch.len_utf8();
        ch
    }
}

fn is_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | ')' | ';')
}

fn looks_numeric(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_digit() => true,
        Some('+' | '-') => chars.next().is_some_and(|ch| ch.is_ascii_digit()),
        _ => false,
    }
}

/// Reader-level symbol grammar. Naming conventions such as kebab-case are a
/// later semantic concern; the reader only admits an unambiguous ASCII set.
///
/// A symbol starts with an ASCII letter, underscore, or hyphen. Its remaining
/// characters may additionally contain digits, dots, slashes, question marks,
/// and exclamation marks. Slash is accepted lexically and reserved from user
/// definitions by later semantic validation.
fn is_symbol(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || matches!(ch, '_' | '-'))
        && chars
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '?' | '!'))
}

fn parse_number(text: &str, span: Span) -> Result<TokenKind, SyntaxError> {
    if text.contains(['.', 'e', 'E']) {
        let value = text.parse::<f64>().map_err(|_| {
            SyntaxError::new(
                "E-SYN-004",
                format!("malformed floating-point literal `{text}`"),
                span,
            )
        })?;
        if !value.is_finite() {
            return Err(SyntaxError::new(
                "E-SYN-004",
                "floating-point literal must be finite",
                span,
            ));
        }
        Ok(TokenKind::Float(value))
    } else {
        text.parse::<i64>().map(TokenKind::Int).map_err(|_| {
            SyntaxError::new(
                "E-SYN-004",
                format!("malformed or out-of-range integer literal `{text}`"),
                span,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_every_token_kind_and_byte_span() {
        let tokens = lex("(hello \"a\\n\\u{03bb}\" -12 +2.5e1 true false unit) ; note").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 1));
        assert_eq!(tokens[1].span, Span::new(1, 6));
        assert_eq!(
            tokens.iter().map(|token| &token.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::OpenParen,
                &TokenKind::Symbol("hello".into()),
                &TokenKind::String("a\nλ".into()),
                &TokenKind::Int(-12),
                &TokenKind::Float(25.0),
                &TokenKind::Bool(true),
                &TokenKind::Bool(false),
                &TokenKind::Unit,
                &TokenKind::CloseParen,
                &TokenKind::Comment(" note".into()),
            ]
        );
    }

    #[test]
    fn delimiters_do_not_need_whitespace() {
        assert_eq!(
            kinds("(a)(b)"),
            vec![
                TokenKind::OpenParen,
                TokenKind::Symbol("a".into()),
                TokenKind::CloseParen,
                TokenKind::OpenParen,
                TokenKind::Symbol("b".into()),
                TokenKind::CloseParen,
            ]
        );
    }

    #[test]
    fn rejects_non_sexpr_delimiters_and_malformed_numbers() {
        for source in ["[a]", "{a}", "a,b"] {
            assert_eq!(lex(source).unwrap_err().code, "E-SYN-001");
        }
        for source in ["1abc", "1.2.3", "9223372036854775808"] {
            assert_eq!(lex(source).unwrap_err().code, "E-SYN-004");
        }
    }

    #[test]
    fn enforces_reader_symbol_characters_without_enforcing_style() {
        assert_eq!(
            kinds("valid-symbol Qualified_Name module.member -private path/name ready? bang!"),
            vec![
                TokenKind::Symbol("valid-symbol".into()),
                TokenKind::Symbol("Qualified_Name".into()),
                TokenKind::Symbol("module.member".into()),
                TokenKind::Symbol("-private".into()),
                TokenKind::Symbol("path/name".into()),
                TokenKind::Symbol("ready?".into()),
                TokenKind::Symbol("bang!".into()),
            ]
        );
        for source in [
            "$legacy",
            "name:",
            "with@sign",
            ".leading",
            "/leading",
            "?leading",
            "!leading",
            "λ",
            "embedded\"quote",
        ] {
            assert_eq!(
                lex(source).unwrap_err().code,
                "E-SYN-001",
                "source: {source}"
            );
        }
    }

    #[test]
    fn reports_string_failures() {
        assert_eq!(lex("\"no end").unwrap_err().code, "E-SYN-002");
        assert_eq!(lex("\"bad\\q\"").unwrap_err().code, "E-SYN-003");
        assert_eq!(lex("\"bad\\0\"").unwrap_err().code, "E-SYN-003");
        assert_eq!(lex("\"bad\\u{}\"").unwrap_err().code, "E-SYN-003");
        assert_eq!(lex("\"bad\nline\"").unwrap_err().code, "E-SYN-002");
    }
}
