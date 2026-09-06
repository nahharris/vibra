//! Reproducible, bounded Milestone 1 fuzz campaign harness.
//!
//! This is an in-tree harness rather than a user-facing command or a
//! dependency on an external fuzzing runner. It deliberately keeps the seed,
//! generator, target list, and budgets visible so a CI smoke run and the
//! separately recorded campaign exercise the same properties at different
//! budgets.

use std::any::Any;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vibra_fmt::format_document;
use vibra_syntax::{
    CstNode, Document, DocumentMode, QueryError, SyntaxKind, canonical_data, lex_bytes,
    parse_data, parse_source,
};

const DEFAULT_SEED: u64 = 0x4d_31_5f_76_31_5f_11;
const DEFAULT_ITERATIONS: usize = 128;
const DEFAULT_DEEP_LIMIT: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    RawBytes,
    Utf8Source,
    Utf8Data,
    RoundTrip,
    Queries,
    CorpusMutation,
}

impl Target {
    const ALL: [Self; 6] = [
        Self::RawBytes,
        Self::Utf8Source,
        Self::Utf8Data,
        Self::RoundTrip,
        Self::Queries,
        Self::CorpusMutation,
    ];

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "raw-bytes" => Ok(Self::RawBytes),
            "utf8-source" => Ok(Self::Utf8Source),
            "utf8-data" => Ok(Self::Utf8Data),
            "roundtrip" => Ok(Self::RoundTrip),
            "queries" => Ok(Self::Queries),
            "corpus-mutation" => Ok(Self::CorpusMutation),
            _ => Err(format!(
                "unknown target `{value}`; expected raw-bytes, utf8-source, utf8-data, roundtrip, queries, or corpus-mutation"
            )),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::RawBytes => "raw-bytes",
            Self::Utf8Source => "utf8-source",
            Self::Utf8Data => "utf8-data",
            Self::RoundTrip => "roundtrip",
            Self::Queries => "queries",
            Self::CorpusMutation => "corpus-mutation",
        }
    }
}

#[derive(Clone, Debug)]
struct Config {
    targets: Vec<Target>,
    iterations: usize,
    seed: u64,
    deep_limit: usize,
    corpus_root: PathBuf,
}

#[derive(Clone, Debug)]
struct Seed {
    path: PathBuf,
    mode: DocumentMode,
    text: String,
}

#[derive(Clone, Copy, Debug)]
struct Lcg {
    state: u64,
}

impl Lcg {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next() as usize) % upper
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("m1-fuzz: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let config = parse_args(std::env::args().skip(1))?;
    let seeds = load_seeds(&config.corpus_root)?;
    let mut executed = 0usize;
    for (target_index, target) in config.targets.iter().copied().enumerate() {
        for iteration in 0..config.iterations {
            let seed = config
                .seed
                .wrapping_add((target_index as u64).wrapping_mul(0x9e37_79b9))
                .wrapping_add(iteration as u64);
            let mut random = Lcg::new(seed);
            let result = catch_unwind(AssertUnwindSafe(|| {
                run_one(target, iteration, &mut random, config.deep_limit, &seeds)
            }));
            let result = match result {
                Ok(result) => result,
                Err(payload) => Err(format!("panic: {}", panic_text(payload))),
            };
            result.map_err(|error| {
                format!(
                    "target={} iteration={} seed=0x{seed:016x}: {error}",
                    target.name(),
                    iteration,
                )
            })?;
            executed = executed.saturating_add(1);
        }
        println!(
            "PASS target={} iterations={} seed=0x{:016x}",
            target.name(),
            config.iterations,
            config
                .seed
                .wrapping_add((target_index as u64).wrapping_mul(0x9e37_79b9)),
        );
    }
    println!(
        "m1 fuzz campaign: {} target(s), {} case(s), deep-limit={}, workers=1",
        config.targets.len(),
        executed,
        config.deep_limit,
    );
    Ok(())
}

fn parse_args<I>(mut args: I) -> Result<Config, String>
where
    I: Iterator<Item = String>,
{
    let mut targets = Vec::new();
    let mut iterations = DEFAULT_ITERATIONS;
    let mut seed = DEFAULT_SEED;
    let mut deep_limit = DEFAULT_DEEP_LIMIT;
    let mut corpus_root = PathBuf::from("conformance/cases");
    let mut smoke = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!(
                    "m1-fuzz [--profile ci-smoke] [--target NAME] [--iterations N] [--seed N] [--deep-limit N] [--corpus-root PATH]"
                );
                return Err("help requested".to_owned());
            }
            "--profile" => {
                let profile = args
                    .next()
                    .ok_or_else(|| "--profile requires a value".to_owned())?;
                if profile != "ci-smoke" {
                    return Err(format!(
                        "unknown profile `{profile}`; only ci-smoke exists"
                    ));
                }
                smoke = true;
            }
            "--target" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--target requires a value".to_owned())?;
                if value == "all" {
                    targets = Target::ALL.to_vec();
                } else {
                    targets = vec![Target::parse(&value)?];
                }
            }
            "--iterations" => {
                iterations = parse_number(&mut args, "--iterations")?;
            }
            "--seed" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--seed requires a value".to_owned())?;
                seed = parse_integer(&value)?;
            }
            "--deep-limit" => {
                deep_limit = parse_number(&mut args, "--deep-limit")?;
            }
            "--corpus-root" => {
                corpus_root = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--corpus-root requires a path".to_owned())?,
                );
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option `{value}`"));
            }
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }

    if smoke {
        iterations = 16;
        deep_limit = 256;
    }
    if targets.is_empty() {
        targets = Target::ALL.to_vec();
    }
    if iterations == 0 {
        return Err("--iterations must be positive".to_owned());
    }
    if deep_limit == 0 {
        return Err("--deep-limit must be positive".to_owned());
    }
    Ok(Config {
        targets,
        iterations,
        seed,
        deep_limit,
        corpus_root,
    })
}

fn parse_number<I>(args: &mut I, option: &str) -> Result<usize, String>
where
    I: Iterator<Item = String>,
{
    let value = args
        .next()
        .ok_or_else(|| format!("{option} requires a value"))?;
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {option} value `{value}`: {error}"))
}

fn parse_integer(value: &str) -> Result<u64, String> {
    value
        .strip_prefix("0x")
        .map_or_else(|| value.parse::<u64>(), |hex| u64::from_str_radix(hex, 16))
        .map_err(|error| format!("invalid seed `{value}`: {error}"))
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn run_one(
    target: Target,
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
    seeds: &[Seed],
) -> Result<(), String> {
    match target {
        Target::RawBytes => raw_bytes_case(iteration, random, deep_limit),
        Target::Utf8Source => source_case(iteration, random, deep_limit),
        Target::Utf8Data => data_case(iteration, random, deep_limit),
        Target::RoundTrip => roundtrip_case(iteration, random),
        Target::Queries => query_case(iteration, random, deep_limit),
        Target::CorpusMutation => {
            corpus_mutation_case(iteration, random, deep_limit, seeds)
        }
    }
}

fn raw_bytes_case(
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
) -> Result<(), String> {
    let mut bytes = generated_source(iteration, random, deep_limit).into_bytes();
    let invalid = iteration.is_multiple_of(2);
    if invalid {
        bytes.push(0xff);
        bytes.extend_from_slice(&[0xc3, 0x28]);
    }
    match lex_bytes(&bytes) {
        Ok(_) if invalid => Err("invalid UTF-8 was accepted by lex_bytes".to_owned()),
        Ok(lexed) => {
            let _ = lexed.tokens();
            Ok(())
        }
        Err(_) if invalid => Ok(()),
        Err(error) => Err(format!("valid UTF-8 was rejected: {error}")),
    }
}

fn source_case(
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
) -> Result<(), String> {
    let source = generated_source(iteration, random, deep_limit);
    let document =
        parse_source("generated.vib", &source).map_err(|error| error.to_string())?;
    assert_lossless(&document, &source)
}

fn data_case(
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
) -> Result<(), String> {
    let source = generated_data(iteration, random, deep_limit);
    let document =
        parse_data("generated.vibon", &source).map_err(|error| error.to_string())?;
    assert_lossless(&document, &source)
}

fn roundtrip_case(iteration: usize, random: &mut Lcg) -> Result<(), String> {
    if iteration.is_multiple_of(2) {
        let source = generated_roundtrip_source(iteration, random);
        let document = parse_source("roundtrip.vib", &source)
            .map_err(|error| error.to_string())?;
        let formatted = format_document(&document);
        if document.recovered() {
            if formatted != source {
                return Err("recovered source was changed by the formatter".to_owned());
            }
            return Ok(());
        }
        let reparsed = parse_source("roundtrip.vib", &formatted)
            .map_err(|error| error.to_string())?;
        if reparsed.recovered() {
            return Err(
                "formatted accepted source became recovered on reparse".to_owned()
            );
        }
        if format_document(&reparsed) != formatted {
            return Err("accepted source formatting was not idempotent".to_owned());
        }
        if structural_signature(document.root())
            != structural_signature(reparsed.root())
        {
            return Err("accepted source lost structural equivalence".to_owned());
        }
        Ok(())
    } else {
        let source = generated_data(iteration, random, 64);
        let document = parse_data("roundtrip.vibon", &source)
            .map_err(|error| error.to_string())?;
        let formatted = format_document(&document);
        if !document.accepted() {
            if document.recovered() && formatted != source {
                return Err(
                    "recovered invalid data was changed by the formatter".to_owned()
                );
            }
            return Ok(());
        }
        if document.recovered() {
            if formatted != source {
                return Err("recovered data was changed by the formatter".to_owned());
            }
            return Ok(());
        }
        let reparsed = parse_data("roundtrip.vibon", &formatted)
            .map_err(|error| error.to_string())?;
        if reparsed.recovered() {
            return Err(
                "formatted accepted data became recovered on reparse".to_owned()
            );
        }
        if format_document(&reparsed) != formatted {
            return Err("accepted data formatting was not idempotent".to_owned());
        }
        let Some(original) = document.data() else {
            return Err("accepted data document had no decoded value".to_owned());
        };
        let Some(result) = reparsed.data() else {
            return Err("reparsed data document had no decoded value".to_owned());
        };
        if canonical_data(original) != canonical_data(result) {
            return Err("accepted data lost decoded structural equivalence".to_owned());
        }
        Ok(())
    }
}

fn query_case(
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
) -> Result<(), String> {
    if iteration.is_multiple_of(2) {
        let source =
            generated_source(iteration.saturating_add(1), random, deep_limit.min(256));
        let document =
            parse_source("query.vib", &source).map_err(|error| error.to_string())?;
        query_every_offset(&document)
    } else {
        let source = generated_data(iteration, random, deep_limit.min(256));
        let document =
            parse_data("query.vibon", &source).map_err(|error| error.to_string())?;
        query_every_offset(&document)
    }
}

fn corpus_mutation_case(
    iteration: usize,
    random: &mut Lcg,
    deep_limit: usize,
    seeds: &[Seed],
) -> Result<(), String> {
    let Some(seed) = seeds.get(if seeds.is_empty() {
        0
    } else {
        iteration % seeds.len()
    }) else {
        return source_case(iteration, random, deep_limit);
    };
    let mutated = mutate(seed.text.as_str(), random);
    let document = match seed.mode {
        DocumentMode::Source => {
            parse_source(&seed.path, &mutated).map_err(|error| error.to_string())?
        }
        DocumentMode::Data => {
            parse_data(&seed.path, &mutated).map_err(|error| error.to_string())?
        }
    };
    assert_lossless(&document, &mutated)?;
    let offset = random.range(mutated.len().saturating_add(2));
    match document.query_position(offset) {
        Ok(query) => {
            if query.span().start() > mutated.len()
                || query.span().end() > mutated.len()
            {
                return Err(
                    "query returned a span outside the mutated input".to_owned()
                );
            }
        }
        Err(QueryError::OffsetOutOfBounds { .. })
        | Err(QueryError::InteriorUtf8Offset { .. })
        | Err(QueryError::NoNode { .. }) => {}
    }
    Ok(())
}

fn assert_lossless(document: &Document, source: &str) -> Result<(), String> {
    if document.root().to_source() != source {
        return Err("CST reconstruction changed input bytes".to_owned());
    }
    Ok(())
}

fn query_every_offset(document: &Document) -> Result<(), String> {
    let source = document.source();
    let offsets = if source.len() <= 4_096 {
        (0..=source.len().saturating_add(1)).collect::<Vec<_>>()
    } else {
        let mut offsets = vec![0, source.len(), source.len().saturating_add(1)];
        for bucket in 0..=512usize {
            let offset = bucket.saturating_mul(source.len()) / 512;
            offsets.push(offset);
            offsets.push(offset.saturating_add(1));
        }
        offsets.sort_unstable();
        offsets.dedup();
        offsets
    };
    for offset in offsets {
        let result = document.query_position(offset);
        if offset > source.len() {
            if !matches!(result, Err(QueryError::OffsetOutOfBounds { .. })) {
                return Err(format!("offset {offset} did not report out of bounds"));
            }
            continue;
        }
        if !source.is_char_boundary(offset) {
            if !matches!(result, Err(QueryError::InteriorUtf8Offset { .. })) {
                return Err(format!(
                    "offset {offset} did not report an interior UTF-8 byte"
                ));
            }
            continue;
        }
        if let Ok(query) = result {
            let span = query.span();
            if span.start() > span.end()
                || span.end() > source.len()
                || !source.is_char_boundary(span.start())
                || !source.is_char_boundary(span.end())
            {
                return Err(format!(
                    "query returned an invalid span at offset {offset}"
                ));
            }
        }
    }
    Ok(())
}

fn structural_signature(root: &CstNode) -> Vec<(SyntaxKind, Option<String>)> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if !matches!(
            node.kind(),
            SyntaxKind::Whitespace | SyntaxKind::LineComment
        ) {
            result.push((node.kind(), node.leaf_text().map(str::to_owned)));
        }
        pending.extend(node.children().iter().rev());
    }
    result
}

fn generated_source(iteration: usize, random: &mut Lcg, deep_limit: usize) -> String {
    match iteration % 12 {
        0 => {
            let depth = 1usize.saturating_add(random.range(deep_limit.min(20_000)));
            format!("{}value{}", "(".repeat(depth), ")".repeat(depth))
        }
        1 => "(alpha ; comment 🌱\r\n  beta)".to_owned(),
        2 => "\"truncated\\u{".to_owned(),
        3 => "(a(b)) ) (gamma".to_owned(),
        4 => {
            let length = 1_024usize.saturating_add(random.range(4_096));
            format!("({} 🌱)", "long-name".repeat(length / 9))
        }
        5 => "(log.write level: @info message: \"hello\" rest: one two)".to_owned(),
        6 => "(café e\u{301} 🌱)".to_owned(),
        7 => "(defn f (value str) str value)".to_owned(),
        8 => "(lambda (value i32) i32 (if value value value))".to_owned(),
        9 => "(match value option.some value option.none void)".to_owned(),
        10 => "(as (array u32) (array.of))".to_owned(),
        _ => "(do (let - expression) (try (call value)))".to_owned(),
    }
}

fn generated_roundtrip_source(iteration: usize, random: &mut Lcg) -> String {
    match iteration % 5 {
        0 => "(alpha beta)".to_owned(),
        1 => "((alpha beta) (gamma delta))".to_owned(),
        2 => "(alpha ; comment\n beta)".to_owned(),
        3 => "(do alpha (if beta gamma delta))".to_owned(),
        _ => format!("(label: value rest: {})", random.range(10)),
    }
}

fn generated_data(iteration: usize, random: &mut Lcg, deep_limit: usize) -> String {
    match iteration % 10 {
        0 => "(record name: @hello root: \"src\")".to_owned(),
        1 => "(array 1u8 2u8 🌱)".to_owned(),
        2 => "(map \"b\" 2 \"a\" 1)".to_owned(),
        3 => "(tuple @one (array true false) void)".to_owned(),
        4 => "(record name: @hello name: @duplicate)".to_owned(),
        5 => "(map \"odd\")".to_owned(),
        6 => "(record root: \"src\" ; keep comment\n name: @hello)".to_owned(),
        7 => "(array (array (array value)))".to_owned(),
        8 => {
            let depth = 1usize.saturating_add(random.range(deep_limit.min(2_000)));
            format!("{}void{}", "(array ".repeat(depth), ")".repeat(depth))
        }
        _ => "\"unterminated".to_owned(),
    }
}

fn mutate(source: &str, random: &mut Lcg) -> String {
    let mut characters = source.chars().collect::<Vec<_>>();
    match random.range(4) {
        0 if !characters.is_empty() => {
            let index = random.range(characters.len());
            if let Some(character) = characters.get_mut(index) {
                *character = match random.range(5) {
                    0 => '(',
                    1 => ')',
                    2 => ';',
                    3 => '🌱',
                    _ => 'x',
                };
            }
        }
        1 => {
            let index = random.range(characters.len().saturating_add(1));
            characters.insert(index, '(');
        }
        2 if !characters.is_empty() => {
            let index = random.range(characters.len());
            characters.remove(index);
        }
        _ => characters.extend([')', ' ', '🌱']),
    }
    characters.into_iter().collect()
}

fn load_seeds(root: &Path) -> Result<Vec<Seed>, String> {
    let mut paths = Vec::new();
    collect_seed_paths(root, &mut paths)?;
    let fuzz_root = Path::new("fuzz/corpus");
    if fuzz_root.is_dir() {
        collect_seed_paths(fuzz_root, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    let mut seeds = Vec::new();
    for path in paths {
        let mode = match path.extension().and_then(|extension| extension.to_str()) {
            Some("vib") => DocumentMode::Source,
            Some("vibon") => DocumentMode::Data,
            _ => continue,
        };
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read seed {}: {error}", path.display()))?;
        seeds.push(Seed { path, mode, text });
    }
    Ok(seeds)
}

fn collect_seed_paths(root: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|error| {
        format!("cannot read seed directory {}: {error}", root.display())
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| {
                format!("cannot read seed entry in {}: {error}", root.display())
            })?
            .path();
        if path.is_dir() {
            collect_seed_paths(&path, paths)?;
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("vib") | Some("vibon")
        ) {
            paths.push(path);
        }
    }
    Ok(())
}
