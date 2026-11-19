//! The reader fuzz driver:
//! `fuzz [--profile ci-smoke] [--target lexer|source|data|format|all]
//! [--iterations N] [--seed N] [--deep-limit N] [--corpus-root PATH]`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Required implementation
//! suites", requires fuzzing of the lexer, parser recovery, and formatting.
//! This is an in-tree, deterministic campaign: a seed and an iteration
//! number fix every input, so a finding is reproduced by its printed seed.
//! Four targets check these properties:
//!
//! - `lexer`: random bytes. No panic; an encoding fault is reported inside
//!   the input; the token texts of valid UTF-8 concatenate to the input.
//! - `source`: keyword soup from the reserved-word table, and mutations of
//!   the corpus's `.vib` inputs. No panic; every byte is in exactly one
//!   leaf; every diagnostic span lies inside the input.
//! - `data`: data-token soup and mutations of the corpus's `.vibon` inputs,
//!   with the same properties.
//! - `format`: mutated corpus inputs. A document with a diagnostic is
//!   returned byte for byte; any other is formatted to a text that reads
//!   with no diagnostic, formats to itself, and keeps the input's tokens up
//!   to the section moves the canonical format makes.
//!
//! Every finding is printed with its target, iteration, and seed, and the
//! run exits 1 when there is one. A finding is closed by a reduced
//! `V2-SRC-READER-*` regression case, never by skipping its seed.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vibra_diagnostics::{ByteSpan, Diagnostic};
use vibra_fmt::format_document;
use vibra_syntax::source::keyword::is_section;
use vibra_syntax::{
    Document, DocumentMode, RESERVED_WORDS, SyntaxKind, check_encoding, lex, parse,
};

/// The seed of a run that names none.
const DEFAULT_SEED: u64 = 0x5652_5f52_4541_4452;
/// The deepest generated nesting of a run that names no limit.
const DEFAULT_DEEP_LIMIT: usize = 20_000;
/// Findings printed per target; the count covers them all.
const PRINTED_FINDINGS: usize = 10;

/// A fuzz target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Lexer,
    Source,
    Data,
    Format,
}

impl Target {
    const ALL: [Self; 4] = [Self::Lexer, Self::Source, Self::Data, Self::Format];

    fn parse(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|target| target.name() == name)
            .ok_or_else(|| {
                format!("unknown target `{name}`; expected lexer, source, data, format, or all")
            })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Lexer => "lexer",
            Self::Source => "source",
            Self::Data => "data",
            Self::Format => "format",
        }
    }

    /// The iterations of the full campaign.
    const fn campaign_iterations(self) -> usize {
        match self {
            Self::Lexer | Self::Source | Self::Data => 100_000,
            Self::Format => 20_000,
        }
    }

    /// The iterations of the `ci-smoke` profile.
    const fn smoke_iterations(self) -> usize {
        match self {
            Self::Lexer | Self::Source | Self::Data => 2_000,
            Self::Format => 500,
        }
    }
}

/// The options of one run.
#[derive(Clone, Debug)]
struct Config {
    targets: Vec<Target>,
    smoke: bool,
    iterations: Option<usize>,
    seed: u64,
    deep_limit: usize,
    corpus_root: PathBuf,
}

/// The seed documents mutations start from.
#[derive(Clone, Debug, Default)]
struct Seeds {
    source: Vec<String>,
    data: Vec<String>,
}

/// A small deterministic generator (SplitMix64).
#[derive(Clone, Copy, Debug)]
struct Random {
    state: u64,
}

impl Random {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    /// A number below `upper`, or 0 when `upper` is 0.
    fn below(&mut self, upper: usize) -> usize {
        let Ok(upper) = u64::try_from(upper) else {
            return 0;
        };
        if upper == 0 {
            return 0;
        }
        usize::try_from(self.next() % upper).unwrap_or(0)
    }

    /// Whether a one-in-`n` event happens.
    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        items.get(self.below(items.len()))
    }
}

/// One failed property.
#[derive(Clone, Debug)]
struct Finding {
    iteration: usize,
    seed: u64,
    message: String,
    input: Vec<u8>,
}

fn main() -> ExitCode {
    match run() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(message) => {
            eprintln!("fuzz: {message}");
            ExitCode::from(2)
        }
    }
}

/// Runs the configured targets and returns the number of findings.
fn run() -> Result<usize, String> {
    let config = parse_options(std::env::args().skip(1))?;
    let seeds = load_seeds(&config.corpus_root)?;
    let mut total_findings = 0usize;
    let mut total_iterations = 0usize;
    for (index, target) in config.targets.iter().copied().enumerate() {
        let iterations = config.iterations.unwrap_or(if config.smoke {
            target.smoke_iterations()
        } else {
            target.campaign_iterations()
        });
        let target_seed = config.seed.wrapping_add(
            u64::try_from(index)
                .unwrap_or(0)
                .wrapping_mul(0x1_0000_0001),
        );
        let mut findings = Vec::new();
        for iteration in 0..iterations {
            let seed = target_seed.wrapping_add(
                u64::try_from(iteration)
                    .unwrap_or(0)
                    .wrapping_mul(0x9e37_79b9),
            );
            let mut random = Random::new(seed);
            let input =
                generate(target, iteration, &mut random, config.deep_limit, &seeds);
            let outcome = catch_unwind(AssertUnwindSafe(|| check(target, &input)));
            let failure = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(message)) => Some(message),
                Err(payload) => {
                    Some(format!("panic: {}", panic_text(payload.as_ref())))
                }
            };
            if let Some(message) = failure {
                findings.push(Finding {
                    iteration,
                    seed,
                    message,
                    input,
                });
            }
        }
        for finding in findings.iter().take(PRINTED_FINDINGS) {
            println!(
                "FINDING target={} iteration={} seed=0x{:016x}: {}\n  input: {}",
                target.name(),
                finding.iteration,
                finding.seed,
                finding.message,
                preview(&finding.input),
            );
        }
        println!(
            "target {}: {iterations} iterations, seed 0x{target_seed:016x}, {} findings",
            target.name(),
            findings.len(),
        );
        total_iterations = total_iterations.saturating_add(iterations);
        total_findings = total_findings.saturating_add(findings.len());
    }
    println!(
        "fuzz: {} targets, {total_iterations} iterations, deep limit {}, {total_findings} findings",
        config.targets.len(),
        config.deep_limit,
    );
    Ok(total_findings)
}

fn parse_options<I>(mut arguments: I) -> Result<Config, String>
where
    I: Iterator<Item = String>,
{
    let mut config = Config {
        targets: Vec::new(),
        smoke: false,
        iterations: None,
        seed: DEFAULT_SEED,
        deep_limit: DEFAULT_DEEP_LIMIT,
        corpus_root: vibra_conformance::workspace_root(),
    };
    let mut deep_limit = None;
    while let Some(argument) = arguments.next() {
        let mut value = |option: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{option} requires a value"))
        };
        match argument.as_str() {
            "--profile" => {
                let profile = value("--profile")?;
                if profile != "ci-smoke" {
                    return Err(format!(
                        "unknown profile `{profile}`; only ci-smoke exists"
                    ));
                }
                config.smoke = true;
            }
            "--target" => {
                let name = value("--target")?;
                config.targets = if name == "all" {
                    Target::ALL.to_vec()
                } else {
                    vec![Target::parse(&name)?]
                };
            }
            "--iterations" => {
                config.iterations =
                    Some(number(&value("--iterations")?, "--iterations")?);
            }
            "--seed" => config.seed = seed(&value("--seed")?)?,
            "--deep-limit" => {
                deep_limit = Some(number(&value("--deep-limit")?, "--deep-limit")?);
            }
            "--corpus-root" => {
                config.corpus_root = PathBuf::from(value("--corpus-root")?)
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`"));
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    if config.targets.is_empty() {
        config.targets = Target::ALL.to_vec();
    }
    config.deep_limit = deep_limit.unwrap_or(if config.smoke {
        256
    } else {
        DEFAULT_DEEP_LIMIT
    });
    if config.iterations == Some(0) || config.deep_limit == 0 {
        return Err("--iterations and --deep-limit must be positive".to_owned());
    }
    Ok(config)
}

fn number(text: &str, option: &str) -> Result<usize, String> {
    text.parse()
        .map_err(|error| format!("invalid {option} value `{text}`: {error}"))
}

fn seed(text: &str) -> Result<u64, String> {
    text.strip_prefix("0x")
        .map_or_else(|| text.parse(), |hex| u64::from_str_radix(hex, 16))
        .map_err(|error| format!("invalid seed `{text}`: {error}"))
}

fn panic_text(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

/// The input, escaped, cut at 400 characters.
fn preview(input: &[u8]) -> String {
    let text = String::from_utf8_lossy(input);
    let escaped: String = text.escape_debug().take(400).collect();
    format!("{} bytes \"{escaped}\"", input.len())
}

// Seeds.

/// Every `.vib` and `.vibon` file of `fuzz/corpus` and `conformance/cases`
/// below `root`.
fn load_seeds(root: &Path) -> Result<Seeds, String> {
    let mut paths = Vec::new();
    for directory in ["fuzz/corpus", "conformance/cases"] {
        collect_documents(&root.join(directory), &mut paths)?;
    }
    paths.sort();
    let mut seeds = Seeds::default();
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            return Err(format!("cannot read the seed {}", path.display()));
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        match DocumentMode::from_path(&path) {
            Some(DocumentMode::Source) => seeds.source.push(text),
            Some(DocumentMode::Data) => seeds.data.push(text),
            None => {}
        }
    }
    if seeds.source.is_empty() || seeds.data.is_empty() {
        return Err(format!("no seed documents below {}", root.display()));
    }
    Ok(seeds)
}

fn collect_documents(directory: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
            .path();
        if path.is_dir() {
            collect_documents(&path, found)?;
        } else if DocumentMode::from_path(&path).is_some() {
            found.push(path);
        }
    }
    Ok(())
}

// Generators.

fn generate(
    target: Target,
    iteration: usize,
    random: &mut Random,
    deep_limit: usize,
    seeds: &Seeds,
) -> Vec<u8> {
    match target {
        Target::Lexer => random_bytes(random),
        Target::Source if iteration % 64 == 63 => {
            deep(random, deep_limit, &SOURCE_NESTS, true)
        }
        Target::Data if iteration % 64 == 63 => {
            deep(random, deep_limit, &DATA_NESTS, false)
        }
        Target::Source if iteration.is_multiple_of(2) => {
            soup(random, &source_vocabulary())
        }
        Target::Data if iteration.is_multiple_of(2) => soup(random, DATA_WORDS),
        Target::Source => {
            let seed = random_seed(random, &seeds.source);
            mutate(random, seed)
        }
        Target::Data => {
            let seed = random_seed(random, &seeds.data);
            mutate(random, seed)
        }
        Target::Format => {
            let pool = if iteration.is_multiple_of(2) {
                &seeds.source
            } else {
                &seeds.data
            };
            let seed = random_seed(random, pool);
            if random.one_in(4) {
                seed.as_bytes().to_vec()
            } else {
                mutate(random, seed)
            }
        }
    }
}

fn random_seed<'a>(random: &mut Random, pool: &'a [String]) -> &'a str {
    random.pick(pool).map_or("", String::as_str)
}

/// The pieces random bytes are drawn from, beside single random bytes.
const BYTE_PIECES: &[&str] = &[
    "\"", "'", "\\", "@", "-", "_", ".", ":", "/", "//", "{", "}", "[", "]", "(", ")",
    " ", "\t", "\r", "\n", "\r\n", "\"\"\"", "\\u{", "0", "7", "1u8", "2.5f64", "1e3",
    "a", "Z", "é", "🌱", "e\u{301}", "\u{feff}", "\0", "fn", "endfn", "void",
];

fn random_bytes(random: &mut Random) -> Vec<u8> {
    let mut bytes = Vec::new();
    if random.one_in(16) {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    let length = if random.one_in(8) {
        random.below(512)
    } else {
        random.below(48)
    };
    for _ in 0..length {
        match random.below(8) {
            0 => bytes.push(u8::try_from(random.below(256)).unwrap_or(0)),
            1 => bytes.push(0x80 | u8::try_from(random.below(64)).unwrap_or(0)),
            _ => {
                if let Some(piece) = random.pick(BYTE_PIECES) {
                    bytes.extend_from_slice(piece.as_bytes());
                }
            }
        }
    }
    bytes
}

/// The words source soup is drawn from: every reserved word and some
/// names, atoms, literals, brackets, comments, and malformed tokens.
fn source_vocabulary() -> Vec<&'static str> {
    let mut words: Vec<&'static str> = RESERVED_WORDS.to_vec();
    words.extend_from_slice(&[
        "f",
        "x",
        "value",
        "T",
        "I32",
        "Self",
        "Fn",
        "Proc",
        "Intent",
        "Intrinsic",
        "true",
        "List",
        "Option.some",
        "a.b",
        "@a",
        "@std.io",
        "@public",
        "@private",
        "@compiler",
        "1i32",
        "-2",
        "1.5f64",
        "\"s\"",
        "'c'",
        "\"\"\"doc\"\"\"",
        "_",
        "(",
        ")",
        "{",
        "}",
        "[",
        "]",
        ":",
        ".",
        "// note\n",
        "my_field",
        "1q",
        "\"open",
        "'",
        "@a.",
        "@1",
        "?",
    ]);
    words
}

/// The words data soup is drawn from.
const DATA_WORDS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    ":",
    "a:",
    "name:",
    "\"k\":",
    "1u8",
    "-7i64",
    "2.5f64",
    "1e3",
    "\"text\"",
    "'x'",
    "@atom",
    "@std.io",
    "true",
    "false",
    "void",
    "name",
    "fn",
    "// c\n",
    "(",
    ")",
    "f(1)",
    "\"\"\"d\"\"\"",
    "my_field",
    "\"open",
    "1q",
    "[:]",
    "{}",
    "[]",
];

/// Separators between soup words; an empty one tests token separation.
const GAPS: &[&str] = &[" ", " ", " ", "\n", "  ", "\r\n", "\t", ""];

fn soup(random: &mut Random, words: &[&str]) -> Vec<u8> {
    let mut text = String::new();
    let count = random.below(40);
    for _ in 0..count {
        if let Some(word) = random.pick(words) {
            text.push_str(word);
        }
        if let Some(gap) = random.pick(GAPS) {
            text.push_str(gap);
        }
    }
    text.into_bytes()
}

/// An opener, a filler, and a closer of one nesting shape.
type Nest = (&'static str, &'static str, &'static str);

const SOURCE_NESTS: [Nest; 4] = [
    ("[", "1i32", "]"),
    ("do ", "1i32", " enddo"),
    ("{", "x", "}"),
    ("f(", "x", ")"),
];

const DATA_NESTS: [Nest; 3] = [("[", "1u8", "]"), ("{", "1u8", "}"), ("[", "", "]")];

/// A deeply nested document, sometimes with a closer missing or extra; a
/// source nest is the initializer of a module value.
fn deep(random: &mut Random, deep_limit: usize, nests: &[Nest], wrap: bool) -> Vec<u8> {
    let Some((open, fill, close)) = random.pick(nests).copied() else {
        return Vec::new();
    };
    let depth = 1 + random.below(deep_limit);
    let mut closers = depth;
    if random.one_in(3) {
        closers = closers.saturating_sub(1 + random.below(3));
    } else if random.one_in(3) {
        closers = closers.saturating_add(1 + random.below(3));
    }
    let mut text = String::new();
    if wrap {
        text.push_str("let x: T be ");
    }
    text.push_str(&open.repeat(depth));
    text.push_str(fill);
    text.push_str(&close.repeat(closers));
    if wrap {
        text.push_str(" endlet");
    }
    text.into_bytes()
}

/// Mutates a valid UTF-8 text at character boundaries.
fn mutate(random: &mut Random, seed: &str) -> Vec<u8> {
    let mut characters: Vec<char> = seed.chars().collect();
    let edits = 1 + random.below(3);
    for _ in 0..edits {
        let length = characters.len();
        let at = random.below(length.saturating_add(1));
        match random.below(6) {
            0 if length > 0 => {
                let end = at.saturating_add(1 + random.below(12)).min(length);
                characters.drain(at.min(end)..end);
            }
            1 => {
                let words = source_vocabulary();
                if let Some(word) = random.pick(&words) {
                    let insert: Vec<char> = format!(" {word} ").chars().collect();
                    characters.splice(at.min(length)..at.min(length), insert);
                }
            }
            2 if length > 0 => {
                let end = at.saturating_add(1 + random.below(24)).min(length);
                let copy: Vec<char> = characters
                    .get(at.min(end)..end)
                    .unwrap_or_default()
                    .to_vec();
                characters.splice(end..end, copy);
            }
            3 if length > 1 => {
                let other = random.below(length);
                if at < length {
                    characters.swap(at, other);
                }
            }
            4 => characters.truncate(at),
            _ => {
                let pieces = [
                    '{', '}', '[', ']', '(', ')', ':', '\n', ' ', '"', '\'', '🌱',
                ];
                if let Some(piece) = random.pick(&pieces) {
                    characters.insert(at.min(length), *piece);
                }
            }
        }
    }
    characters.into_iter().collect::<String>().into_bytes()
}

// Properties.

fn check(target: Target, input: &[u8]) -> Result<(), String> {
    match target {
        Target::Lexer => check_lexer(input),
        Target::Source => {
            check_reader(&parse(DocumentMode::Source, None, input), input)
        }
        Target::Data => check_reader(&parse(DocumentMode::Data, None, input), input),
        Target::Format => check_format(input),
    }
}

fn check_lexer(input: &[u8]) -> Result<(), String> {
    let (text, diagnostics) = check_encoding(input);
    spans_inside(&diagnostics, input.len(), None)?;
    let valid = std::str::from_utf8(input).is_ok();
    let Some(text) = text else {
        if valid {
            return Err("valid UTF-8 has no text".to_owned());
        }
        if diagnostics.is_empty() {
            return Err("invalid UTF-8 has no encoding diagnostic".to_owned());
        }
        return Ok(());
    };
    if !valid || text.as_bytes() != input {
        return Err("the text of the document is not its bytes".to_owned());
    }
    let lexed = lex(text);
    let mut end = 0;
    let mut joined = String::with_capacity(text.len());
    for token in &lexed.tokens {
        if token.span.start() != end || token.span.is_empty() {
            return Err(format!(
                "the token at {} is not contiguous or is empty",
                end
            ));
        }
        let Some(piece) = token.span.text(text) else {
            return Err(format!(
                "the token {:?} is not on character boundaries",
                token.span
            ));
        };
        joined.push_str(piece);
        end = token.span.end();
    }
    if joined != text {
        return Err("the token texts do not concatenate to the input".to_owned());
    }
    spans_inside(&lexed.diagnostics, input.len(), Some(text))
}

/// Every byte is in exactly one leaf, a missing leaf is empty, and every
/// diagnostic span lies inside the input.
fn check_reader(document: &Document, input: &[u8]) -> Result<(), String> {
    spans_inside(document.diagnostics(), input.len(), document.text())?;
    let Some(text) = document.text() else {
        if std::str::from_utf8(input).is_ok() {
            return Err("valid UTF-8 has no text".to_owned());
        }
        return Ok(());
    };
    let Some(root) = document.root() else {
        return Err("a document with text has no tree".to_owned());
    };
    let mut end = 0;
    for leaf in root.leaves() {
        let span = leaf.span();
        if span.start() != end {
            return Err(format!(
                "a leaf starts at {} where {} was expected",
                span.start(),
                end
            ));
        }
        if span.is_empty() != (leaf.kind() == SyntaxKind::Missing) {
            return Err(format!(
                "the leaf at {} is empty exactly when it is not missing",
                end
            ));
        }
        end = span.end();
    }
    if end != text.len() {
        return Err(format!("the leaves end at {end} of {}", text.len()));
    }
    Ok(())
}

fn check_format(input: &[u8]) -> Result<(), String> {
    let Some(mode) = [DocumentMode::Source, DocumentMode::Data]
        .into_iter()
        .find(|mode| parse(*mode, None, input).diagnostics().is_empty())
        .or(Some(DocumentMode::Source))
    else {
        return Ok(());
    };
    for mode in [mode, other(mode)] {
        let document = parse(mode, None, input);
        check_reader(&document, input)?;
        let formatted = format_document(&document);
        let (Some(text), Some(once)) = (document.text(), formatted) else {
            if document.text().is_some() {
                return Err("a document with text is not formatted".to_owned());
            }
            continue;
        };
        if !document.diagnostics().is_empty() {
            if once != text {
                return Err("a recovered document was changed".to_owned());
            }
            continue;
        }
        let reparsed = parse(mode, None, once.as_bytes());
        if !reparsed.diagnostics().is_empty() {
            return Err(format!(
                "the formatted text reads with {:?}",
                reparsed
                    .diagnostics()
                    .iter()
                    .map(Diagnostic::code)
                    .collect::<Vec<_>>()
            ));
        }
        if format_document(&reparsed).as_deref() != Some(once.as_str()) {
            return Err("formatting is not idempotent".to_owned());
        }
        same_tokens(text, &once)?;
    }
    Ok(())
}

const fn other(mode: DocumentMode) -> DocumentMode {
    match mode {
        DocumentMode::Source => DocumentMode::Data,
        DocumentMode::Data => DocumentMode::Source,
    }
}

/// The sections the formatter removes when their brace is empty.
const REMOVABLE_WHEN_EMPTY: [&str; 4] = ["takes", "labelled", "given", "performs"];

/// The output holds the input's tokens: the same sequence, or, where a
/// declaration's sections or members moved, the same tokens in another
/// order. An empty removable section may be gone.
fn same_tokens(input: &str, output: &str) -> Result<(), String> {
    let significant = |text: &'_ str| -> Vec<String> {
        lex(text)
            .tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .map(|token| token.span.text(text).unwrap_or_default().to_owned())
            .collect()
    };
    let written = significant(input);
    let mut before = Vec::new();
    let mut index = 0;
    while let Some(token) = written.get(index) {
        let empty = REMOVABLE_WHEN_EMPTY.contains(&token.as_str())
            && written.get(index + 1).is_some_and(|next| next == "{")
            && written.get(index + 2).is_some_and(|next| next == "}");
        if empty {
            index += 3;
        } else {
            before.push(token.clone());
            index += 1;
        }
    }
    let after = significant(output);
    if before == after {
        return Ok(());
    }
    if !before.iter().any(|token| is_section(token)) {
        return Err("the token order changed where no section can move".to_owned());
    }
    let mut sorted_before = before;
    sorted_before.sort_unstable();
    let mut sorted_after = after;
    sorted_after.sort_unstable();
    if sorted_before == sorted_after {
        Ok(())
    } else {
        Err("formatting added or lost a token".to_owned())
    }
}

/// Every primary and related span is ordered, inside `length`, and, when
/// the text is known, on character boundaries.
fn spans_inside(
    diagnostics: &[Diagnostic],
    length: usize,
    text: Option<&str>,
) -> Result<(), String> {
    let inside = |span: ByteSpan| {
        span.start() <= span.end()
            && span.end() <= length
            && text.is_none_or(|text| span.text(text).is_some())
    };
    for diagnostic in diagnostics {
        if !inside(diagnostic.primary_span()) {
            return Err(format!(
                "{} has the span {:?} outside the input of {length} bytes",
                diagnostic.code().as_atom(),
                diagnostic.primary_span()
            ));
        }
        if let Some(related) = diagnostic
            .related()
            .iter()
            .find(|related| !inside(related.span))
        {
            return Err(format!(
                "{} has the related span {:?} outside the input",
                diagnostic.code().as_atom(),
                related.span
            ));
        }
    }
    Ok(())
}
