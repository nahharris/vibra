//! Step 11's specification-example inventory and lossless exercise.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use vibra_syntax::{DocumentMode, parse_data, parse_source};

const INVENTORY_PATH: &str = "docs/roadmap/milestone-1/syntax-examples.tsv";
const INVENTORY_HEADER: &str = "kind\tpath\tstart\tend\theading\texcerpt\tmode\tclassification\tevidence\tcontext\tdeferred\tcount\tdigest";
const ALL_CLASSIFICATIONS: &str =
    "reader-positive|reader-negative|recovery|non-source grammar/schema illustrations";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fence {
    path: String,
    start: usize,
    end: usize,
    heading: String,
    language: String,
    excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Inline {
    line: usize,
    ordinal: usize,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Section {
    path: String,
    start: usize,
    end: usize,
    heading: String,
    inline: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InventoryRow {
    kind: String,
    path: String,
    start: usize,
    end: usize,
    heading: String,
    excerpt: String,
    mode: String,
    classification: String,
    evidence: String,
    context: String,
    deferred: String,
    count: usize,
    digest: String,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn source_documents(root: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(root.join("docs/spec"))
        .expect("active specification directory")
        .map(|entry| entry.expect("readable specification entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn heading(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let marker_end = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if (1..=6).contains(&marker_end)
        && trimmed.as_bytes().get(marker_end) == Some(&b' ')
    {
        Some(trimmed[marker_end.saturating_add(1)..].to_owned())
    } else {
        None
    }
}

fn is_fence_line(line: &str) -> bool {
    line.starts_with("```")
}

fn normalized_excerpt(lines: &[String]) -> String {
    let mut excerpt = lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .replace('\t', " ");
    if excerpt.chars().count() > 80 {
        excerpt = excerpt.chars().take(80).collect();
    }
    excerpt
}

fn inline_spans(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut remainder = line;
    while let Some(start) = remainder.find('`') {
        remainder = &remainder[start.saturating_add(1)..];
        let Some(end) = remainder.find('`') else {
            break;
        };
        let text = &remainder[..end];
        if !text.contains('`') {
            spans.push(text.to_owned());
        }
        remainder = &remainder[end.saturating_add(1)..];
    }
    spans
}

fn scan_document(root: &Path, path: &Path) -> (Vec<Fence>, Vec<Section>) {
    let relative = path
        .strip_prefix(root)
        .expect("specification path belongs to the workspace")
        .to_string_lossy()
        .replace('\\', "/");
    let lines = fs::read_to_string(path)
        .expect("read active specification")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(value) = heading(line) {
            headings.push((index.saturating_add(1), value));
        }
    }

    let mut fences = Vec::new();
    let mut inline_by_line = BTreeMap::<usize, Vec<Inline>>::new();
    let mut inside_fence = false;
    let mut fence_start = 0;
    let mut language = String::new();
    let mut body = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let line_number = index.saturating_add(1);
        if !inside_fence && is_fence_line(line) {
            inside_fence = true;
            fence_start = line_number;
            language = line
                .strip_prefix("```")
                .unwrap_or_default()
                .trim()
                .to_owned();
            body.clear();
            continue;
        }
        if inside_fence && is_fence_line(line) {
            let current_heading = headings
                .iter()
                .rev()
                .find(|(start, _)| *start < fence_start)
                .map_or_else(String::new, |(_, value)| value.clone());
            fences.push(Fence {
                path: relative.clone(),
                start: fence_start,
                end: line_number,
                heading: current_heading,
                language: language.clone(),
                excerpt: normalized_excerpt(&body),
            });
            inside_fence = false;
            body.clear();
            continue;
        }
        if inside_fence {
            body.push(line.clone());
            continue;
        }
        let spans = inline_spans(line);
        if !spans.is_empty() {
            inline_by_line.insert(
                line_number,
                spans
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, text)| Inline {
                        line: line_number,
                        ordinal: ordinal.saturating_add(1),
                        text,
                    })
                    .collect(),
            );
        }
    }

    let mut sections = Vec::new();
    for (index, (start, value)) in headings.iter().enumerate() {
        let end = headings
            .get(index.saturating_add(1))
            .map_or(lines.len(), |(next, _)| next.saturating_sub(1));
        let inline = inline_by_line
            .range(*start..=end)
            .flat_map(|(_, values)| values.iter().cloned())
            .collect::<Vec<_>>();
        if !inline.is_empty() {
            sections.push(Section {
                path: relative.clone(),
                start: *start,
                end,
                heading: value.clone(),
                inline,
            });
        }
    }
    (fences, sections)
}

fn fnv_digest(inline: &[Inline]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for entry in inline {
        let canonical = format!("{}\t{}\t{}\n", entry.line, entry.ordinal, entry.text);
        for byte in canonical.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

fn parse_inventory(root: &Path) -> Vec<InventoryRow> {
    let path = root.join(INVENTORY_PATH);
    let text = fs::read_to_string(path).expect("tracked syntax-example inventory");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(INVENTORY_HEADER),
        "inventory header changed"
    );
    lines
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                13,
                "inventory rows have 13 tab-separated fields"
            );
            InventoryRow {
                kind: fields[0].to_owned(),
                path: fields[1].to_owned(),
                start: fields[2].parse().expect("inventory start line"),
                end: fields[3].parse().expect("inventory end line"),
                heading: fields[4].to_owned(),
                excerpt: fields[5].to_owned(),
                mode: fields[6].to_owned(),
                classification: fields[7].to_owned(),
                evidence: fields[8].to_owned(),
                context: fields[9].to_owned(),
                deferred: fields[10].to_owned(),
                count: fields[11].parse().expect("inventory count"),
                digest: fields[12].to_owned(),
            }
        })
        .collect()
}

fn is_source_inline(text: &str) -> bool {
    text.contains('(')
        || text.contains(')')
        || text.contains('\\')
        || text.split_whitespace().count() > 1
        || text.starts_with('@')
        || text.starts_with("-:")
        || text.starts_with("-.")
        || matches!(text, "void" | "true" | "false" | "any")
        || text
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
}

fn exercise_source_fragment(text: &str) {
    let document =
        parse_source("spec-example.vib", text).expect("source fragment mode");
    assert_eq!(
        document.root().to_source(),
        text,
        "source fragment lost bytes"
    );
}

fn exercise_fence(fence: &Fence, body: &str) {
    match fence.language.as_str() {
        "vibra" => {
            let document =
                parse_source("spec-example.vib", body).expect("source fence mode");
            assert_eq!(document.root().to_source(), body, "source fence lost bytes");
        }
        "vibon" => {
            let document =
                parse_data("spec-example.vibon", body).expect("data fence mode");
            assert_eq!(document.root().to_source(), body, "data fence lost bytes");
        }
        _ => {}
    }
}

fn expected_fence_row(fence: &Fence) -> String {
    let (mode, classification, evidence) = match fence.language.as_str() {
        "vibra" => (
            "source",
            "reader-positive",
            "spec_examples_are_losslessly_exercised",
        ),
        "vibon" => (
            "data",
            "reader-positive",
            "spec_examples_are_losslessly_exercised",
        ),
        _ => (
            "non-source",
            "non-source grammar/schema illustrations",
            "review-only:grammar-or-schema-illustration",
        ),
    };
    format!(
        "fence\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\texact fenced fragment\tsemantic/type/effect/runtime claims remain deferred where applicable\t1\t-",
        fence.path,
        fence.start,
        fence.end,
        fence.heading,
        fence.excerpt,
        mode,
        classification,
        evidence,
    )
}

fn expected_section_row(section: &Section) -> String {
    format!(
        "inline\t{}\t{}\t{}\t{}\t{}\tinline\t{}\tspec_examples_are_losslessly_exercised\tisolated UTF-8 source fragments; section heading supplies context\tsemantic/type/effect/runtime claims remain deferred; reader and recovery facts are exercised\t{}\t{}",
        section.path,
        section.start,
        section.end,
        section.heading,
        section.heading,
        ALL_CLASSIFICATIONS,
        section.inline.len(),
        fnv_digest(&section.inline),
    )
}

#[test]
fn spec_examples_are_inventoried_and_losslessly_exercised() {
    let root = workspace_root();
    let inventory = parse_inventory(&root);
    let mut fences = Vec::new();
    let mut sections = Vec::new();
    let mut fence_bodies = BTreeMap::new();
    for path in source_documents(&root) {
        let (found_fences, found_sections) = scan_document(&root, &path);
        let lines = fs::read_to_string(&path)
            .expect("read specification for fence exercise")
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for fence in &found_fences {
            let body = lines[fence.start..fence.end.saturating_sub(1)].join("\n");
            fence_bodies.insert((fence.path.clone(), fence.start), body);
        }
        fences.extend(found_fences);
        sections.extend(found_sections);
    }

    let expected_fences = fences
        .iter()
        .map(|fence| ((fence.path.clone(), fence.start), expected_fence_row(fence)))
        .collect::<BTreeMap<(String, usize), String>>();
    let expected_sections = sections
        .iter()
        .map(|section| {
            (
                (section.path.clone(), section.start),
                expected_section_row(section),
            )
        })
        .collect::<BTreeMap<(String, usize), String>>();
    let actual_keys = inventory
        .iter()
        .map(|row| (row.kind.clone(), row.path.clone(), row.start))
        .collect::<BTreeSet<_>>();
    let expected_keys = expected_fences
        .keys()
        .map(|(path, line)| ("fence".to_owned(), path.clone(), *line))
        .chain(
            expected_sections
                .keys()
                .map(|(path, line)| ("inline".to_owned(), path.clone(), *line)),
        )
        .collect::<BTreeSet<_>>();
    let mut failures = Vec::new();
    for missing in expected_keys.difference(&actual_keys) {
        let row = if missing.0 == "fence" {
            expected_fences
                .get(&(missing.1.clone(), missing.2))
                .expect("expected fence row")
                .clone()
        } else {
            expected_sections
                .get(&(missing.1.clone(), missing.2))
                .expect("expected inline row")
                .clone()
        };
        failures.push(format!("missing inventory row: {row}"));
    }
    for extra in actual_keys.difference(&expected_keys) {
        failures.push(format!(
            "stale inventory row: {}\t{}\t{}",
            extra.0, extra.1, extra.2
        ));
    }

    for row in &inventory {
        if row.kind == "fence" {
            let Some(fence) = fences
                .iter()
                .find(|fence| fence.path == row.path && fence.start == row.start)
            else {
                continue;
            };
            let expected = expected_fence_row(fence);
            let actual = format!(
                "fence\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.path,
                row.start,
                row.end,
                row.heading,
                row.excerpt,
                row.mode,
                row.classification,
                row.evidence,
                row.context,
                row.deferred,
                row.count,
                row.digest,
            );
            if actual != expected {
                failures.push(format!(
                    "stale fence inventory row: {actual}\nexpected: {expected}"
                ));
            }
        } else if row.kind == "inline" {
            let Some(section) = sections
                .iter()
                .find(|section| section.path == row.path && section.start == row.start)
            else {
                continue;
            };
            let expected = expected_section_row(section);
            let actual = format!(
                "inline\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.path,
                row.start,
                row.end,
                row.heading,
                row.excerpt,
                row.mode,
                row.classification,
                row.evidence,
                row.context,
                row.deferred,
                row.count,
                row.digest,
            );
            if actual != expected {
                failures.push(format!(
                    "stale inline inventory row: {actual}\nexpected: {expected}"
                ));
            }
        } else {
            failures.push(format!("unknown inventory row kind: {}", row.kind));
        }
    }

    for fence in &fences {
        let body = fence_bodies
            .get(&(fence.path.clone(), fence.start))
            .expect("fence body indexed by stable location");
        exercise_fence(fence, body);
    }
    for section in &sections {
        for inline in &section.inline {
            if is_source_inline(&inline.text) {
                exercise_source_fragment(&inline.text);
            }
        }
    }

    assert!(
        failures.is_empty(),
        "specification example inventory is stale or incomplete:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        fences.len(),
        45,
        "active spec fence count changed; review inventory"
    );
    assert!(
        sections
            .iter()
            .map(|section| section.inline.len())
            .sum::<usize>()
            > 100,
        "inline inventory unexpectedly became empty"
    );
}

#[test]
fn deeply_nested_source_and_data_survive_traversal_and_drop() {
    const SOURCE_DEPTH: usize = 20_000;
    let source_text = format!(
        "{}value{}",
        "(".repeat(SOURCE_DEPTH),
        ")".repeat(SOURCE_DEPTH)
    );
    let source = parse_source("deep.vib", &source_text)
        .expect("deep source selects source mode");
    assert_eq!(source.mode(), DocumentMode::Source);
    assert!(source.accepted());
    assert_eq!(source.root().to_source(), source_text);
    drop(source);

    const DATA_DEPTH: usize = 2_000;
    let data_text = format!(
        "{}@value{}",
        "(array ".repeat(DATA_DEPTH),
        ")".repeat(DATA_DEPTH)
    );
    let data =
        parse_data("deep.vibon", &data_text).expect("deep data selects data mode");
    assert!(data.accepted());
    assert!(data.data().is_some());
    assert_eq!(data.root().to_source(), data_text);
    drop(data);
}
