//! The owner audit of the M1 exit.
//!
//! `docs/roadmap/v0.0.2/m0/owners.md` names, for every supported form, the
//! conformance cases planned for it and its diagnostics. This test reads that
//! table, the grammar of `docs/spec/01-source-language.md`, the diagnostic
//! tables of `docs/spec/07-diagnostics-and-conformance.md`, the rule index,
//! and the committed corpus, and fails on every gap at once:
//!
//! - a case identity a row names that the corpus does not have, or a `*`
//!   pattern that matches no case;
//! - an M1 form with no accepted case, with no case that expects a diagnostic
//!   when the row names one, or, for a grammar production, with no case that
//!   compares formatter output;
//! - a production of chapter 01's grammar with no owner row, or with no
//!   accepted and no rejected case;
//! - a discharge rule `GUARD-n` or integer fact `INT-n` without an accepted
//!   and a rejected or nearest-failing case;
//! - a row of chapter 07's diagnostic tables whose codes M1 owns, none of
//!   whose codes a case expects, or none of whose owner rows has an accepted
//!   case;
//! - a code M1 owns that no case expects.
//!
//! An M1 form is a row whose step column names an M1 step. A row that names
//! no case identity at all is owned by host tests, which the row names, and
//! is listed by [`audit`] rather than checked against the corpus. There is
//! no exemption list.

use std::collections::{BTreeMap, BTreeSet};

use vibra_conformance::{
    ConformanceOperation, Corpus, RuleIndex, RuleStatus, Verdict, corpus_root,
    workspace_root,
};

/// One case of the corpus, as the audit reads it.
#[derive(Debug)]
struct CaseFacts {
    rule: String,
    verdict: Verdict,
    codes: BTreeSet<String>,
    operation: ConformanceOperation,
    /// For a case that compares formatter output, the forms its input holds.
    formatted: Option<BTreeSet<String>>,
    description: String,
}

impl CaseFacts {
    fn accepted(&self) -> bool {
        self.verdict == Verdict::Accepted
    }

    fn rejected(&self) -> bool {
        self.verdict != Verdict::Accepted && !self.codes.is_empty()
    }
}

/// One row of a table of the owner document.
#[derive(Debug)]
struct OwnerRow {
    section: String,
    form: String,
    step: String,
    patterns: Vec<String>,
    codes: Vec<String>,
    /// The cases cell names host tests for part of the form.
    host: bool,
    /// The cases cell says `every `interpret` case`.
    every_interpret: bool,
    /// The host tests the cases cell names, as `module::function`.
    host_tests: Vec<String>,
}

impl OwnerRow {
    /// A row is M1's when an M1 step stands before any later milestone's
    /// name: `07 / 23; M2 07, 21` is, `M2 21 / M2 22` and `M3` are not.
    fn is_m1(&self) -> bool {
        self.step
            .split('M')
            .next()
            .is_some_and(|m1| m1.chars().any(|c| c.is_ascii_digit()))
    }

    fn label(&self) -> String {
        format!("{} / {}", self.section, self.form)
    }
}

/// The cells of a Markdown table row, split at the bars outside code spans.
fn cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.starts_with('|') {
        return None;
    }
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    for c in line.chars().skip(1) {
        match c {
            '`' => {
                in_code = !in_code;
                current.push(c);
            }
            '|' if !in_code => {
                cells.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    Some(cells)
}

fn is_separator(cells: &[String]) -> bool {
    cells
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
}

/// The code spans of `text`, in order.
fn code_spans(text: &str) -> Vec<&str> {
    text.split('`')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, span)| span)
        .collect()
}

/// The registry codes written in `text`, in order, each once.
fn codes_in(text: &str, registry: &BTreeSet<String>) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(offset) = text.get(index..).and_then(|rest| rest.find('@')) {
        let start = index + offset;
        let mut end = start + 1;
        while end < bytes.len()
            && bytes
                .get(end)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'.')
        {
            end += 1;
        }
        let candidate = text.get(start..end).unwrap_or("").trim_end_matches('.');
        if registry.contains(candidate) && !found.iter().any(|code| code == candidate) {
            found.push(candidate.to_owned());
        }
        index = end.max(start + 1);
    }
    found
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(workspace_root().join(relative))
        .unwrap_or_else(|error| panic!("`{relative}` reads: {error}"))
        .replace("\r\n", "\n")
}

/// The codes of the registry table of chapter 07.
fn registry(chapter: &str) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    for line in chapter.lines() {
        if line.starts_with("## ") && !codes.is_empty() {
            break;
        }
        if let Some(cells) = cells(line)
            && let [code, level] = cells.as_slice()
            && level.starts_with("`@")
        {
            codes.insert(code.trim_matches('`').to_owned());
        }
    }
    codes
}

/// Every row of the owner document's tables, and the owner of every code.
fn owner_rows(
    document: &str,
    registry: &BTreeSet<String>,
) -> (Vec<OwnerRow>, BTreeMap<String, String>) {
    let mut rows = Vec::new();
    let mut owners = BTreeMap::new();
    let mut section = String::new();
    let mut header_seen = false;
    for line in document.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_owned();
            header_seen = false;
            continue;
        }
        let Some(cells) = cells(line) else {
            header_seen = false;
            continue;
        };
        if !header_seen {
            header_seen = true;
            continue;
        }
        if is_separator(&cells) {
            continue;
        }
        if section == "Diagnostic codes" {
            if let [code, owner] = cells.as_slice() {
                owners.insert(code.trim_matches('`').to_owned(), owner.clone());
            }
            continue;
        }
        let [form, step, cases, diagnostics, ..] = cells.as_slice() else {
            continue;
        };
        let patterns = code_spans(cases)
            .into_iter()
            .filter(|span| span.starts_with("V1-") || span.starts_with("V2-"))
            .map(str::to_owned)
            .collect();
        rows.push(OwnerRow {
            section: section.clone(),
            form: form.clone(),
            step: step.clone(),
            patterns,
            codes: codes_in(diagnostics, registry),
            host: cases.contains("host test"),
            every_interpret: cases.contains("every `interpret` case"),
            host_tests: code_spans(cases)
                .into_iter()
                .filter(|span| is_test_path(span))
                .map(str::to_owned)
                .collect(),
        });
    }
    (rows, owners)
}

/// The production names chapter 01's two `ebnf` blocks define.
fn productions(chapter: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_ebnf = false;
    for line in chapter.lines() {
        if line.starts_with("```ebnf") {
            in_ebnf = true;
            continue;
        }
        if line.starts_with("```") {
            in_ebnf = false;
            continue;
        }
        if !in_ebnf || line.starts_with(' ') {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim();
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// The rows of chapter 07's diagnostic tables: the condition and its codes.
fn chapter_rows(
    chapter: &str,
    registry: &BTreeSet<String>,
) -> Vec<(String, String, Vec<String>)> {
    const SECTIONS: &[&str] = &[
        "### Availability, discovery and resolution",
        "### Syntax and data diagnostics",
        "### Type and interface diagnostics",
        "### Binding, return and pattern diagnostics",
        "### Contract, effect and authority diagnostics",
        "### Runtime and tool failures",
    ];
    let mut rows = Vec::new();
    let mut section: Option<&str> = None;
    let mut header_seen = false;
    for line in chapter.lines() {
        if line.starts_with('#') {
            section = SECTIONS.iter().copied().find(|name| line.trim() == *name);
            header_seen = false;
            continue;
        }
        let Some(name) = section else { continue };
        let Some(cells) = cells(line) else {
            header_seen = false;
            continue;
        };
        if !header_seen {
            header_seen = true;
            continue;
        }
        if is_separator(&cells) {
            continue;
        }
        if let [condition, code, ..] = cells.as_slice() {
            rows.push((
                name.trim_start_matches("### ").to_owned(),
                condition.clone(),
                codes_in(code, registry),
            ));
        }
    }
    rows
}

/// The forms a source holds, for the formatter evidence of a production: the
/// `kind` atom of every node of its `@tree.v2` observation, `return`, `try`
/// and `await` as written, and `@armLiteral` for a literal in a `match` arm.
fn source_forms(text: &str) -> BTreeSet<String> {
    let document = vibra_syntax::parse_source(None, text.as_bytes());
    let mut forms = BTreeSet::new();
    let Some(root) = document.root() else {
        return forms;
    };
    let mut pending = vec![(root, false)];
    while let Some((node, in_arm)) = pending.pop() {
        let kind = node.kind();
        if let Some(atom) = kind.tree_kind() {
            forms.insert(atom.to_owned());
        }
        if kind == vibra_syntax::SyntaxKind::ReservedWord {
            forms.insert(node.text(text).to_owned());
        }
        let literal = matches!(
            kind,
            vibra_syntax::SyntaxKind::Integer
                | vibra_syntax::SyntaxKind::Float
                | vibra_syntax::SyntaxKind::String
                | vibra_syntax::SyntaxKind::Character
                | vibra_syntax::SyntaxKind::Atom
        );
        if literal && in_arm {
            forms.insert("@armLiteral".to_owned());
        }
        let in_arm = in_arm || kind == vibra_syntax::SyntaxKind::Arm;
        pending.extend(node.children().iter().map(|child| (child, in_arm)));
    }
    forms
}

/// The forms whose presence in a formatter case's input shows that the
/// formatter laid out a production. A production that is a node of the
/// `@tree.v2` observation is shown by that node (`fn-decl` by `@fnDecl`);
/// these are the productions that are not nodes.
const FORMAT_EVIDENCE: &[(&str, &[&str])] = &[
    ("module", &["@source"]),
    ("top", &["@source"]),
    ("type-body", &["@typeDecl"]),
    ("type-item", &["@typeDecl"]),
    ("statement", &["@body"]),
    ("expr", &["@body"]),
    ("unary", &["return", "try", "await"]),
    ("argument", &["@postfix"]),
    ("primary", &["@postfix"]),
    (
        "literal",
        &["@integer", "@float", "@string", "@character", "@atom"],
    ),
    ("pattern", &["@arm", "@let"]),
    ("pattern-literal", &["@armLiteral"]),
    ("type-expr", &["@signature"]),
];

/// The node atom of a production: `fn-decl` is `@fnDecl`.
fn node_atom(production: &str) -> String {
    let mut atom = String::from("@");
    let mut upper = false;
    for c in production.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            atom.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            atom.push(c);
        }
    }
    atom
}

/// Whether a formatter case whose input holds `forms` shows `production`.
fn shows(production: &str, forms: &BTreeSet<String>) -> bool {
    match FORMAT_EVIDENCE.iter().find(|(name, _)| *name == production) {
        Some((_, evidence)) => evidence.iter().any(|form| forms.contains(*form)),
        None => forms.contains(&node_atom(production)),
    }
}

/// The facts of every case of the committed corpus, by identity. The codes a
/// case expects are those of its diagnostics, its host event, and those its
/// expected documents write.
fn corpus_facts(registry: &BTreeSet<String>) -> BTreeMap<String, CaseFacts> {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    let mut facts = BTreeMap::new();
    for case in corpus.cases() {
        let manifest = case.manifest();
        let expect = &manifest.expectations;
        let mut codes: BTreeSet<String> = expect
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_atom().to_owned())
            .collect();
        if let Some(event) = expect.host_event.as_ref() {
            codes.insert(event.as_atom().to_owned());
        }
        for document in [&expect.types, &expect.result, &expect.test_run]
            .into_iter()
            .flatten()
        {
            let text = case
                .read_file(document)
                .expect("an expected document reads");
            codes.extend(codes_in(&text, registry));
        }
        let compares_format = expect.formatted.is_some()
            || manifest.operation == ConformanceOperation::Format;
        let formatted = compares_format.then(|| {
            manifest
                .inputs
                .source
                .as_deref()
                .and_then(|source| case.read_file(source).ok())
                .map(|text| source_forms(&text))
                .unwrap_or_default()
        });
        facts.insert(
            manifest.id.clone(),
            CaseFacts {
                rule: manifest.rule.clone(),
                operation: manifest.operation,
                verdict: expect.verdict,
                codes,
                formatted,
                description: manifest.description.clone().unwrap_or_default(),
            },
        );
    }
    facts
}

/// The identities a pattern names: every identity that begins with the text
/// before a trailing `*`; otherwise the identity itself and every identity
/// that extends it by a `-` part, as a twin extends the case it varies
/// (`V2-CONTRACT-int2-nearest-failing` of `V2-CONTRACT-int2`).
fn matches<'a>(pattern: &str, cases: &'a BTreeMap<String, CaseFacts>) -> Vec<&'a str> {
    cases
        .keys()
        .filter(|id| matches_id(pattern, id))
        .map(String::as_str)
        .collect()
}

/// `true` when the pattern names `id`, as [`matches`] reads it.
fn matches_id(pattern: &str, id: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => id.starts_with(prefix),
        None => {
            id == pattern
                || id
                    .strip_prefix(pattern)
                    .is_some_and(|rest| rest.starts_with('-'))
        }
    }
}

/// Every case identity or pattern the step guides of M2 write in a code
/// span.
fn later_case_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut pending = vec![workspace_root().join("docs/roadmap/v0.0.2/m2")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("the M2 plan reads") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                let text = std::fs::read_to_string(&path).expect("an M2 guide reads");
                for span in code_spans(&text) {
                    if span.starts_with("V2-") || span.starts_with("V1-") {
                        ids.insert(span.to_owned());
                    }
                }
            }
        }
    }
    ids
}

/// `true` when `text` names `rule` as a word: `GUARD-1` and not `GUARD-12`.
fn names_rule(text: &str, rule: &str) -> bool {
    text.match_indices(rule).any(|(at, _)| {
        !text
            .get(at + rule.len()..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// `true` for a host test's name as an owner row writes it:
/// `module::function`, in snake case.
fn is_test_path(span: &str) -> bool {
    span.split_once("::").is_some_and(|(module, function)| {
        [module, function].iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    })
}

/// Every `module::function` of the workspace's crates: a `fn` written in a
/// file `module.rs`, or in a `mod module` block, under `crates/`.
fn host_test_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = vec![workspace_root().join("crates")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("a crate directory reads") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    pending.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a Rust file reads");
            let mut module = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_owned();
            for line in text.lines() {
                let line = line.trim_start();
                if let Some(rest) = line
                    .strip_prefix("mod ")
                    .or_else(|| line.strip_prefix("pub mod "))
                {
                    if let Some(name) = rest.strip_suffix(" {") {
                        module = name.to_owned();
                    }
                    continue;
                }
                let Some(rest) = line
                    .strip_prefix("fn ")
                    .or_else(|| line.strip_prefix("pub fn "))
                else {
                    continue;
                };
                let function: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                names.insert(format!("{module}::{function}"));
                if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                    names.insert(format!("{stem}::{function}"));
                }
            }
        }
    }
    names
}

/// What the audit found.
#[derive(Default)]
struct Findings {
    gaps: Vec<String>,
    host: Vec<String>,
    deferred: Vec<String>,
    summary: Vec<String>,
}

/// The audit.
#[allow(clippy::too_many_lines)]
fn audit() -> Findings {
    let chapter_07 = read("docs/spec/07-diagnostics-and-conformance.md");
    let chapter_01 = read("docs/spec/01-source-language.md");
    let owners_document = read("docs/roadmap/v0.0.2/m0/owners.md");
    let rules = RuleIndex::from_path(workspace_root().join("conformance/rules.tsv"))
        .expect("the rule index loads");
    let registry = registry(&chapter_07);
    let (rows, owners) = owner_rows(&owners_document, &registry);
    let cases = corpus_facts(&registry);
    let later = later_case_ids();
    let defined = productions(&chapter_01);
    let mut findings = Findings::default();

    let m1_code = |code: &str| {
        owners
            .get(code)
            .is_some_and(|owner| owner.starts_with("steps"))
    };

    // The cases of a discharge rule or an integer fact: those its owner row
    // names, and every case whose rule has `GUARD-n`'s identity prefix
    // `V2-CONTRACT-rule{n}-` (`INT-n`'s `V2-CONTRACT-int{n}`) or whose
    // description names the rule.
    let rule_cases = |rule: &str| -> BTreeSet<&str> {
        let (prefix, form) = match rule.strip_prefix("GUARD-") {
            Some(n) => (format!("V2-CONTRACT-rule{n}-"), format!("{rule} ")),
            None => (
                format!("V2-CONTRACT-int{}", rule.trim_start_matches("INT-")),
                rule.to_owned(),
            ),
        };
        let mut found = BTreeSet::new();
        for row in rows.iter().filter(|row| {
            let name = row.form.replace('`', "");
            name == form.trim() || name.starts_with(&form)
        }) {
            for pattern in &row.patterns {
                found.extend(matches(pattern, &cases));
            }
        }
        for (id, case) in &cases {
            let active = rules.status(&case.rule) == Some(RuleStatus::Active);
            if active
                && (case.rule.starts_with(&prefix)
                    || names_rule(&case.description, rule))
            {
                found.insert(id.as_str());
            }
        }
        found
    };

    // Owner rows. Every host test a row names exists.
    let tests = host_test_names();
    let mut named_tests = 0usize;
    let mut checked = 0usize;
    for row in rows.iter().filter(|row| row.is_m1()) {
        let label = row.label();
        for test in &row.host_tests {
            named_tests += 1;
            if !tests.contains(test) {
                findings.gaps.push(format!(
                    "{label}: no host test `{test}`, which the row names"
                ));
            }
        }
        if row.patterns.is_empty() {
            findings.host.push(format!("{label}: every side"));
            continue;
        }
        checked += 1;
        let form = row.form.trim_matches('`');
        let production = defined.iter().any(|name| name == form);
        let mut named: BTreeSet<&str> = BTreeSet::new();
        let mut row_deferred = false;
        for pattern in &row.patterns {
            let found = matches(pattern, &cases);
            if found.is_empty() {
                if later
                    .iter()
                    .any(|id| matches_id(pattern, id) || matches_id(id, pattern))
                {
                    findings.deferred.push(format!("{label}: `{pattern}`"));
                    row_deferred = true;
                } else if row.host {
                    findings.host.push(format!("{label}: `{pattern}`"));
                } else {
                    findings.gaps.push(format!("{label}: no case `{pattern}`"));
                }
            }
            named.extend(found);
        }
        if row.every_interpret {
            named.extend(
                cases
                    .iter()
                    .filter(|(_, case)| {
                        case.operation == ConformanceOperation::Interpret
                    })
                    .map(|(id, _)| id.as_str()),
            );
        }
        if form.starts_with("GUARD-") || form.starts_with("INT-") {
            let rule = form.split_whitespace().next().unwrap_or(form);
            named.extend(rule_cases(rule));
        }
        let facts: Vec<&CaseFacts> =
            named.iter().filter_map(|id| cases.get(*id)).collect();
        let mut side = |missing: bool, what: &str, deferrable: bool| {
            if !missing {
                return;
            }
            if row.host {
                findings.host.push(format!("{label}: the {what} side"));
            } else if deferrable && row_deferred {
                findings.deferred.push(format!("{label}: the {what} side"));
            } else {
                findings.gaps.push(format!("{label}: no {what} case"));
            }
        };
        side(!facts.iter().any(|case| case.accepted()), "accepted", false);
        let has_m1_code = row.codes.iter().any(|code| m1_code(code));
        side(
            (has_m1_code || production) && !facts.iter().any(|case| case.rejected()),
            "rejected",
            !has_m1_code,
        );
        if row.section == "Grammar productions" {
            let named_format = facts.iter().any(|case| case.formatted.is_some());
            let shown = cases
                .values()
                .filter_map(|case| case.formatted.as_ref())
                .any(|forms| shows(form, forms));
            side(!named_format && !shown, "formatter", false);
        }
    }
    findings.summary.push(format!(
        "owner rows of M1 checked against the corpus: {checked}"
    ));
    findings.summary.push(format!(
        "host tests named by owner rows, each found: {named_tests}"
    ));

    // Chapter 01 productions.
    let production_rows: BTreeSet<&str> = rows
        .iter()
        .filter(|row| {
            row.section == "Grammar productions" || row.section == "Lexical productions"
        })
        .map(|row| row.form.trim_matches('`'))
        .collect();
    for name in &defined {
        if !production_rows.contains(name.as_str()) {
            findings
                .gaps
                .push(format!("chapter 01 production `{name}` has no owner row"));
        }
    }
    findings.summary.push(format!(
        "chapter 01 productions, each with an owner row: {}",
        defined.len()
    ));

    // Discharge rules and integer facts.
    for rule in (1..=7)
        .map(|n| format!("GUARD-{n}"))
        .chain((1..=8).map(|n| format!("INT-{n}")))
    {
        let found = rule_cases(&rule);
        let accepted: Vec<&str> = found
            .iter()
            .copied()
            .filter(|id| cases.get(*id).is_some_and(CaseFacts::accepted))
            .collect();
        let rejected: Vec<&str> = found
            .iter()
            .copied()
            .filter(|id| cases.get(*id).is_some_and(|case| !case.accepted()))
            .collect();
        if accepted.is_empty() {
            findings.gaps.push(format!("{rule}: no accepted case"));
        }
        if rejected.is_empty() {
            findings
                .gaps
                .push(format!("{rule}: no rejected or nearest-failing case"));
        }
        findings.summary.push(format!(
            "{rule}: accepted {}; rejected {}",
            accepted.join(" "),
            rejected.join(" ")
        ));
    }

    // The registry: a code M1 owns is expected by a case, or an owner row of
    // M1 that cites it names host tests.
    let host_code = |code: &str| {
        rows.iter().any(|row| {
            row.is_m1() && row.host && row.codes.iter().any(|cited| cited == code)
        })
    };
    let expected = |code: &str| cases.values().any(|case| case.codes.contains(code));
    let mut m1_codes = 0usize;
    for code in owners.keys().filter(|code| m1_code(code)) {
        m1_codes += 1;
        if !expected(code) {
            if host_code(code) {
                findings.host.push(format!("the code {code}"));
            } else {
                findings
                    .gaps
                    .push(format!("the M1 code {code} is expected by no case"));
            }
        }
    }
    findings
        .summary
        .push(format!("registry codes M1 owns: {m1_codes}"));

    // Chapter 07 rows: one case expects a code of the row, and an owner row
    // citing a code of the row has an accepted case.
    let mut m1_rows = 0usize;
    for (section, condition, codes) in chapter_rows(&chapter_07, &registry) {
        if codes.is_empty() || !codes.iter().all(|code| m1_code(code)) {
            continue;
        }
        m1_rows += 1;
        let label = format!("chapter 07, {section}: {condition}");
        if !codes.iter().any(|code| expected(code)) {
            if codes.iter().all(|code| host_code(code)) {
                findings.host.push(format!("{label}: the rejected side"));
            } else {
                findings
                    .gaps
                    .push(format!("{label}: no case expects {codes:?}"));
            }
        }
        let citing: Vec<&OwnerRow> = rows
            .iter()
            .filter(|row| {
                row.is_m1() && row.codes.iter().any(|code| codes.contains(code))
            })
            .collect();
        // A twin of a case that expects a code of the row, as `matches`
        // reads a twin, is a nearest-passing case of the row too.
        let twin = cases
            .iter()
            .filter(|(_, case)| codes.iter().any(|code| case.codes.contains(code)))
            .any(|(failing, _)| {
                cases.iter().any(|(id, case)| {
                    case.accepted()
                        && (matches_id(failing, id) || matches_id(id, failing))
                })
            });
        let accepted = twin
            || citing.iter().any(|row| {
                row.patterns
                    .iter()
                    .flat_map(|pattern| matches(pattern, &cases))
                    .any(|id| cases.get(id).is_some_and(CaseFacts::accepted))
            });
        if !accepted {
            if citing.iter().any(|row| row.host) {
                findings.host.push(format!("{label}: the accepted side"));
            } else {
                findings.gaps.push(format!(
                    "{label}: no owner row of {codes:?} has an accepted case"
                ));
            }
        }
    }
    findings
        .summary
        .push(format!("chapter 07 rows whose codes M1 owns: {m1_rows}"));
    findings
        .summary
        .push(format!("corpus cases: {}", cases.len()));
    findings
}

#[test]
fn owner_coverage() {
    let findings = audit();
    for line in &findings.summary {
        println!("{line}");
    }
    println!(
        "owned by the host tests their owner row names: {}",
        findings.host.len()
    );
    for line in &findings.host {
        println!("  {line}");
    }
    println!("named by a step guide of M2: {}", findings.deferred.len());
    for line in &findings.deferred {
        println!("  {line}");
    }
    assert!(
        findings.gaps.is_empty(),
        "{} gaps:\n{}",
        findings.gaps.len(),
        findings.gaps.join("\n")
    );
}
