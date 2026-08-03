use std::fmt;

pub const MAX_PROGRAM_WORDS: usize = 0x10000;

pub fn parse_hex_program(source: &str) -> Result<Vec<u32>, ProgramError> {
    let mut words = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let content = line.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if content.split_whitespace().count() != 1 {
            return Err(ProgramError::MultipleWords(line_number));
        }
        let token = content;
        let (digits, radix) = match token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            Some(value) => (value, 16),
            None => (token, 10),
        };
        let word = u32::from_str_radix(digits, radix).map_err(|_| ProgramError::InvalidWord {
            line: line_number,
            token: token.to_string(),
        })?;
        if words.len() == MAX_PROGRAM_WORDS {
            return Err(ProgramError::TooManyWords(line_number));
        }
        words.push(word);
    }
    Ok(words)
}

#[derive(Debug, Eq, PartialEq)]
pub enum ProgramError {
    InvalidWord { line: usize, token: String },
    MultipleWords(usize),
    TooManyWords(usize),
}

impl fmt::Display for ProgramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWord { line, token } => {
                write!(
                    formatter,
                    "invalid instruction word `{token}` on line {line}"
                )
            }
            Self::MultipleWords(line) => write!(
                formatter,
                "line {line} contains more than one instruction word"
            ),
            Self::TooManyWords(line) => write!(
                formatter,
                "program exceeds the 65536-word limit at line {line}"
            ),
        }
    }
}

impl std::error::Error for ProgramError {}
