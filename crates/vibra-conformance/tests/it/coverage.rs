//! The coverage audit of a milestone's exit.
//!
//! The owner table, `docs/roadmap/v0.0.2/m0/owners.md`, names for every
//! supported form the step that owns it, its conformance cases, and its
//! diagnostics. [`World`] reads that table, the chapters it cites, the rule
//! index, the committed corpus, and the functions of the workspace's crates
//! once; [`audit`] checks one milestone against them, given that milestone's
//! [`Milestone`] data, and reports every gap at once. Each milestone is one
//! test in a module of its own, which holds its data.
//!
//! For every owner row whose step column names the milestone:
//!
//! - every host test the row names exists: a `module::function`, or an
//!   `ATK-` identity, which names the functions its suffix spells in snake
//!   case after `atk_`;
//! - a row that names no case identity names host tests;
//! - every case identity or `*` pattern the row names matches a case;
//! - the row has an accepted case; a rejected case when it cites a
//!   diagnostic or is a production of chapter 01; and, in the grammar
//!   productions, a case that compares formatter output.
//!
//! A side a row lacks is held by host tests when the row names host tests.
//! For the milestone, as its data asks:
//!
//! - every code the owner table gives it is a registry code expected by a
//!   case, or is listed with the host test whose text produces it;
//! - every production of chapter 01 has an owner row, and every row of
//!   chapter 07's diagnostic tables whose codes it owns has a case that
//!   expects one of them and an accepted case ([`Milestone::chapters`]);
//! - every discharge rule it lists has an accepted and a rejected case, and
//!   is recorded on a passed obligation of an accepted contract case;
//! - every identity of its authority argument and of its items of required
//!   coverage resolves to a case or a host test;
//! - the cases that expect `@tool.unavailable` are exactly those it lists,
//!   and none is its form.

mod m1;
mod m2;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use vibra_conformance::{
    Case, ConformanceOperation, Corpus, RuleIndex, RuleStatus, Verdict, corpus_root,
    workspace_root,
};
use vibra_types::{ObligationStatus, check_source};

use crate::support::rust_sources;

/// A map of required coverage: each item's words and the identities that
/// hold it.
pub(super) type CoverageMap = &'static [(&'static str, &'static [&'static str])];

/// What a milestone's audit reads beside the owner table.
pub(super) struct Milestone {
    /// The milestone's name, as the step column writes it.
    pub name: &'static str,
    /// Whether an owner row's step column names the milestone.
    pub owns_step: fn(&str) -> bool,
    /// Whether the owner cell of a code of "Diagnostic codes" names it.
    pub owns_code: fn(&str) -> bool,
    /// The expected documents, by manifest key, whose codes a case expects
    /// beside those of its diagnostics and its host event.
    pub documents: &'static [&'static str],
    /// Whether a row that names host tests also holds a case identity of
    /// the row that matches no case, and a code the milestone owns that no
    /// case expects and that the row cites.
    pub host_rows_hold_gaps: bool,
    /// Whether the milestone owns the grammar of chapter 01 and audits the
    /// diagnostic tables of chapter 07 row by row.
    pub chapters: bool,
    /// The discharge rules and integer facts the milestone delivers.
    pub discharge_rules: &'static [&'static str],
    /// A `V2-` identity that is a host test because no case can observe it:
    /// the identity, the host test, and why.
    pub host_only: &'static [(&'static str, &'static str, &'static str)],
    /// A code the milestone owns that no case can expect: the code, the host
    /// test that produces it, the text by which that test asserts it, and
    /// why.
    pub host_codes:
        &'static [(&'static str, &'static str, &'static str, &'static str)],
    /// The authority argument: its document and the number of its rows.
    pub authority: Option<(&'static str, usize)>,
    /// Items of required coverage of chapter 07, by number.
    pub coverage: &'static [(u32, CoverageMap)],
    /// The cases that expect `@tool.unavailable`: the case, the form it
    /// holds, and that form's owner.
    pub unavailable: Option<&'static [(&'static str, &'static str, &'static str)]>,
}

impl Milestone {
    /// Whether the audit reads the codes of `source`: `diagnostics`,
    /// `hostEvent`, or the key of an expected document.
    fn reads(&self, source: &str) -> bool {
        matches!(source, "diagnostics" | "hostEvent")
            || self.documents.contains(&source)
    }
}

/// The expected documents whose codes a case can expect.
const DOCUMENTS: &[&str] = &["types", "result", "testRun", "targetRun"];

/// One case of the corpus, as the audit reads it.
#[derive(Debug)]
struct CaseFacts {
    rule: String,
    verdict: Verdict,
    operation: ConformanceOperation,
    /// Every code the case expects, with where it is written: `diagnostics`,
    /// `hostEvent`, or the key of the expected document that writes it.
    codes: BTreeMap<String, BTreeSet<&'static str>>,
    /// For a case that compares formatter output, the forms its input holds.
    formatted: Option<BTreeSet<String>>,
    description: String,
}

impl CaseFacts {
    fn accepted(&self) -> bool {
        self.verdict == Verdict::Accepted
    }
}

/// One row of a table of the owner document.
#[derive(Debug)]
struct OwnerRow {
    section: String,
    form: String,
    step: String,
    /// The case identities and patterns the cases cell names.
    patterns: Vec<String>,
    /// The registry codes the diagnostics cell cites.
    codes: Vec<String>,
    /// The cases cell says it names host tests.
    host: bool,
    /// The cases cell says `every `interpret` case`.
    every_interpret: bool,
    /// The host tests the cases cell names, as `module::function`.
    host_tests: Vec<String>,
    /// The `ATK-` identities the cases cell names.
    attacks: Vec<String>,
}

impl OwnerRow {
    fn label(&self) -> String {
        format!("{} / {}", self.section, self.form)
    }

    /// Whether the row names host tests, in words or by name.
    fn names_host_tests(&self) -> bool {
        self.host || !self.attacks.is_empty() || !self.host_tests.is_empty()
    }
}

/// The owner document's rows and the owner of every code.
pub(super) struct OwnerTable {
    rows: Vec<OwnerRow>,
    code_owners: BTreeMap<String, String>,
}

impl OwnerTable {
    pub(super) fn parse(document: &str, registry: &BTreeSet<String>) -> Self {
        let mut rows = Vec::new();
        let mut code_owners = BTreeMap::new();
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
                    code_owners
                        .insert(code.trim_matches('`').to_owned(), owner.clone());
                }
                continue;
            }
            let [form, step, cases, diagnostics, ..] = cells.as_slice() else {
                continue;
            };
            let spans = code_spans(cases);
            let named = |keep: fn(&str) -> bool| {
                spans
                    .iter()
                    .copied()
                    .filter(|span| keep(span))
                    .map(str::to_owned)
                    .collect()
            };
            rows.push(OwnerRow {
                section: section.clone(),
                form: form.clone(),
                step: step.clone(),
                patterns: named(|span| {
                    span.starts_with("V1-") || span.starts_with("V2-")
                }),
                codes: codes_in(diagnostics, registry),
                host: cases.contains("host test"),
                every_interpret: cases.contains("every `interpret` case"),
                host_tests: named(is_test_path),
                attacks: named(|span| span.starts_with("ATK-")),
            });
        }
        Self { rows, code_owners }
    }
}

/// What every audit reads, read once.
pub(super) struct World {
    chapter_07: String,
    registry: BTreeSet<String>,
    /// The production names of chapter 01's grammar.
    productions: Vec<String>,
    /// The text of the owner document.
    owners: String,
    table: OwnerTable,
    rules: RuleIndex,
    corpus: Corpus,
    cases: BTreeMap<String, CaseFacts>,
    /// Every `module::function` of the workspace's crates.
    tests: BTreeSet<String>,
    /// Every `fn` of the workspace's crates, by name, with its files.
    functions: BTreeMap<String, Vec<PathBuf>>,
}

impl World {
    pub(super) fn get() -> &'static Self {
        static WORLD: OnceLock<World> = OnceLock::new();
        WORLD.get_or_init(Self::load)
    }

    fn load() -> Self {
        let chapter_07 = read("docs/spec/07-diagnostics-and-conformance.md");
        let registry = registry(&chapter_07);
        let owners = read("docs/roadmap/v0.0.2/m0/owners.md");
        let table = OwnerTable::parse(&owners, &registry);
        let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
        let cases = corpus_facts(&corpus, &registry);
        let (tests, functions) = workspace_functions();
        Self {
            productions: productions(&read("docs/spec/01-source-language.md")),
            rules: RuleIndex::from_root(workspace_root().join("conformance"))
                .expect("the rule index loads"),
            chapter_07,
            registry,
            owners,
            table,
            corpus,
            cases,
            tests,
            functions,
        }
    }

    pub(super) fn owners(&self) -> &str {
        &self.owners
    }

    pub(super) fn registry(&self) -> &BTreeSet<String> {
        &self.registry
    }
}

/// What an audit found.
#[derive(Default, Debug)]
pub(super) struct Findings {
    pub gaps: Vec<String>,
    /// What host tests hold, as the row says.
    pub host: Vec<String>,
    pub summary: Vec<String>,
}

impl Findings {
    fn print(&self) {
        for line in &self.summary {
            println!("{line}");
        }
        println!("held by host tests, as the row says: {}", self.host.len());
        for line in &self.host {
            println!("  {line}");
        }
    }
}

/// Audits `milestone`, prints what it found, and fails on every gap.
pub(super) fn assert_covered(milestone: &Milestone) {
    let findings = audit(milestone);
    findings.print();
    assert!(
        findings.gaps.is_empty(),
        "{} gaps:\n{}",
        findings.gaps.len(),
        findings.gaps.join("\n")
    );
}

/// Every check of `milestone`.
pub(super) fn audit(milestone: &Milestone) -> Findings {
    let world = World::get();
    let audit = Audit { world, milestone };
    let mut findings = audit.rows(&world.table);
    audit.codes(&mut findings);
    if milestone.chapters {
        audit.productions(&mut findings);
        audit.chapter_rows(&mut findings);
    }
    audit.discharge_rules(&mut findings);
    audit.host_only(&mut findings);
    audit.authority(&mut findings);
    for (number, map) in milestone.coverage {
        audit.coverage(*number, map, &mut findings);
    }
    audit.unavailable(&mut findings);
    findings
        .summary
        .push(format!("corpus cases: {}", world.cases.len()));
    findings
}

/// One milestone's view of the world.
#[derive(Clone, Copy)]
pub(super) struct Audit<'w> {
    pub world: &'w World,
    pub milestone: &'w Milestone,
}

impl<'w> Audit<'w> {
    /// Whether `case` expects `code`, as the milestone reads its codes.
    fn expects(self, case: &CaseFacts, code: &str) -> bool {
        case.codes.get(code).is_some_and(|sources| {
            sources.iter().any(|source| self.milestone.reads(source))
        })
    }

    /// Whether `case` is refuted, unknown or unsupported and expects a code.
    fn rejected(self, case: &CaseFacts) -> bool {
        !case.accepted()
            && case.codes.values().any(|sources| {
                sources.iter().any(|source| self.milestone.reads(source))
            })
    }

    /// Whether some case expects `code`.
    fn expected(self, code: &str) -> bool {
        self.world
            .cases
            .values()
            .any(|case| self.expects(case, code))
    }

    /// The cases a pattern names, as [`matches_id`] reads one.
    fn matching(self, pattern: &str) -> Vec<&'w str> {
        self.world
            .cases
            .keys()
            .filter(|id| matches_id(pattern, id))
            .map(String::as_str)
            .collect()
    }

    /// Whether a row of the milestone names host tests and cites `code`.
    fn host_code(self, table: &OwnerTable, code: &str) -> bool {
        table.rows.iter().any(|row| {
            (self.milestone.owns_step)(&row.step)
                && row.names_host_tests()
                && row.codes.iter().any(|cited| cited == code)
        })
    }

    /// The host tests an `ATK-` identity or pattern names: `ATK-forged-token`
    /// is `atk_forged_token` or a test whose name extends it by `_`, and
    /// `ATK-fs-*` every test whose name begins with `atk_fs_`.
    fn attack_tests(self, id: &str) -> Vec<&'w str> {
        let stem = id
            .trim_start_matches("ATK-")
            .trim_end_matches('*')
            .replace('-', "_");
        let name = format!("atk_{stem}");
        self.world
            .functions
            .keys()
            .filter(|function| {
                if id.ends_with('*') {
                    function.starts_with(&name)
                } else {
                    **function == name || function.starts_with(&format!("{name}_"))
                }
            })
            .map(String::as_str)
            .collect()
    }

    /// Whether an identity of a coverage map or the authority argument
    /// resolves: a `V2-` case of the corpus, a `V2-` identity of
    /// [`Milestone::host_only`] whose host test exists, or an `ATK-` host
    /// test.
    fn resolves(self, id: &str) -> Result<String, String> {
        if id.starts_with("ATK-") {
            let found = self.attack_tests(id);
            return if found.is_empty() {
                Err(format!("no host test `{id}`"))
            } else {
                Ok(format!("host test {}", found.join(", ")))
            };
        }
        if self.world.cases.contains_key(id) {
            return Ok("case".to_owned());
        }
        match self
            .milestone
            .host_only
            .iter()
            .find(|(only, ..)| *only == id)
        {
            Some((_, test, _)) if self.world.functions.contains_key(*test) => {
                Ok(format!("host test {test}"))
            }
            Some((_, test, _)) => Err(format!("`{id}`: no host test `{test}`")),
            None => Err(format!("no case `{id}`")),
        }
    }

    /// The cases of a discharge rule or an integer fact: those its owner row
    /// names, and every case of an active rule whose rule has `GUARD-n`'s
    /// identity prefix `V2-CONTRACT-rule{n}-` (`INT-n`'s `V2-CONTRACT-int{n}`)
    /// or whose description names the rule.
    fn rule_cases(self, table: &OwnerTable, rule: &str) -> BTreeSet<&'w str> {
        let (prefix, form) = match rule.strip_prefix("GUARD-") {
            Some(n) => (format!("V2-CONTRACT-rule{n}-"), format!("{rule} ")),
            None => (
                format!("V2-CONTRACT-int{}", rule.trim_start_matches("INT-")),
                rule.to_owned(),
            ),
        };
        let mut found = BTreeSet::new();
        for row in table.rows.iter().filter(|row| {
            let name = row.form.replace('`', "");
            name == form.trim() || name.starts_with(&form)
        }) {
            for pattern in &row.patterns {
                found.extend(self.matching(pattern));
            }
        }
        for (id, case) in &self.world.cases {
            let active =
                self.world.rules.status(&case.rule) == Some(RuleStatus::Active);
            if active
                && (case.rule.starts_with(&prefix)
                    || names_rule(&case.description, rule))
            {
                found.insert(id.as_str());
            }
        }
        found
    }

    /// The owner rows of the milestone.
    pub(super) fn rows(self, table: &OwnerTable) -> Findings {
        let mut findings = Findings::default();
        let mut checked = 0usize;
        let mut named_tests = 0usize;
        for row in table
            .rows
            .iter()
            .filter(|row| (self.milestone.owns_step)(&row.step))
        {
            checked += 1;
            let label = row.label();
            for test in &row.host_tests {
                named_tests += 1;
                if !self.world.tests.contains(test) {
                    findings.gaps.push(format!(
                        "{label}: no host test `{test}`, which the row names"
                    ));
                }
            }
            for attack in &row.attacks {
                named_tests += 1;
                if self.attack_tests(attack).is_empty() {
                    findings.gaps.push(format!(
                        "{label}: no host test `{attack}`, which the row names"
                    ));
                }
            }
            if row.patterns.is_empty() {
                if row.names_host_tests() {
                    findings.host.push(format!("{label}: every side"));
                } else {
                    findings
                        .gaps
                        .push(format!("{label}: names no case and no host test"));
                }
                continue;
            }
            let form = row.form.trim_matches('`');
            let mut named: BTreeSet<&str> = BTreeSet::new();
            for pattern in &row.patterns {
                let found = self.matching(pattern);
                if found.is_empty() {
                    if self.milestone.host_rows_hold_gaps && row.names_host_tests() {
                        findings.host.push(format!("{label}: `{pattern}`"));
                    } else {
                        findings.gaps.push(format!("{label}: no case `{pattern}`"));
                    }
                }
                named.extend(found);
            }
            if row.every_interpret {
                named.extend(
                    self.world
                        .cases
                        .iter()
                        .filter(|(_, case)| {
                            case.operation == ConformanceOperation::Interpret
                        })
                        .map(|(id, _)| id.as_str()),
                );
            }
            if form.starts_with("GUARD-") || form.starts_with("INT-") {
                let rule = form.split_whitespace().next().unwrap_or(form);
                named.extend(self.rule_cases(table, rule));
            }
            let facts: Vec<&CaseFacts> = named
                .iter()
                .filter_map(|id| self.world.cases.get(*id))
                .collect();
            let mut side = |missing: bool, what: &str| {
                if !missing {
                    return;
                }
                if row.names_host_tests() {
                    findings.host.push(format!("{label}: the {what} side"));
                } else {
                    findings.gaps.push(format!("{label}: no {what} case"));
                }
            };
            side(!facts.iter().any(|case| case.accepted()), "accepted");
            let production = self.world.productions.iter().any(|name| name == form);
            side(
                (!row.codes.is_empty() || production)
                    && !facts.iter().any(|case| self.rejected(case)),
                "rejected",
            );
            if row.section == "Grammar productions" {
                let named_format = facts.iter().any(|case| case.formatted.is_some());
                let shown = self
                    .world
                    .cases
                    .values()
                    .filter_map(|case| case.formatted.as_ref())
                    .any(|forms| shows(form, forms));
                side(!named_format && !shown, "formatter");
            }
        }
        let name = self.milestone.name;
        findings
            .summary
            .push(format!("owner rows of {name} checked: {checked}"));
        findings.summary.push(format!(
            "host tests named by owner rows of {name}, each found: {named_tests}"
        ));
        findings
    }

    /// Every code the owner table gives the milestone is expected by a case.
    fn codes(self, findings: &mut Findings) {
        let table = &self.world.table;
        let name = self.milestone.name;
        let owned: Vec<&str> = table
            .code_owners
            .iter()
            .filter(|(code, owner)| {
                code.starts_with('@') && (self.milestone.owns_code)(owner)
            })
            .map(|(code, _)| code.as_str())
            .collect();
        if owned.is_empty() {
            findings
                .gaps
                .push(format!("the owner table gives no code to {name}"));
        }
        for code in &owned {
            if !self.world.registry.contains(*code) {
                findings.gaps.push(format!("{code} is not a registry code"));
            }
            let expecting: Vec<&str> = self
                .world
                .cases
                .iter()
                .filter(|(_, case)| self.expects(case, code))
                .map(|(id, _)| id.as_str())
                .collect();
            let listed = self
                .milestone
                .host_codes
                .iter()
                .find(|(held, ..)| held == code);
            match (expecting.first(), listed) {
                (Some(first), None) => findings.summary.push(format!(
                    "{code}: {first} and {} more",
                    expecting.len() - 1
                )),
                (None, Some((_, test, needle, reason))) => {
                    let Some(paths) = self.world.functions.get(*test) else {
                        findings.gaps.push(format!("{code}: no host test `{test}`"));
                        continue;
                    };
                    if paths
                        .iter()
                        .any(|path| function_text(path, test).contains(needle))
                    {
                        findings
                            .summary
                            .push(format!("{code}: host test {test}: {reason}"));
                    } else {
                        findings
                            .gaps
                            .push(format!("{code}: host test `{test}` does not produce it"));
                    }
                }
                (Some(first), Some(_)) => findings.gaps.push(format!(
                    "{code} is listed as held by a host test, but the case {first} expects it: \
                     remove the entry"
                )),
                (None, None) => {
                    if self.milestone.host_rows_hold_gaps && self.host_code(table, code) {
                        findings.host.push(format!("the code {code}"));
                    } else {
                        findings
                            .gaps
                            .push(format!("the {name} code {code} is expected by no case"));
                    }
                }
            }
        }
        findings
            .summary
            .push(format!("registry codes {name} owns: {}", owned.len()));
    }

    /// Every production of chapter 01 has an owner row.
    fn productions(self, findings: &mut Findings) {
        let rows: BTreeSet<&str> = self
            .world
            .table
            .rows
            .iter()
            .filter(|row| {
                row.section == "Grammar productions"
                    || row.section == "Lexical productions"
            })
            .map(|row| row.form.trim_matches('`'))
            .collect();
        for name in &self.world.productions {
            if !rows.contains(name.as_str()) {
                findings
                    .gaps
                    .push(format!("chapter 01 production `{name}` has no owner row"));
            }
        }
        findings.summary.push(format!(
            "chapter 01 productions, each with an owner row: {}",
            self.world.productions.len()
        ));
    }

    /// Every row of chapter 07's diagnostic tables whose codes the
    /// milestone owns: a case expects a code of the row, and an owner row
    /// citing a code of the row has an accepted case.
    fn chapter_rows(self, findings: &mut Findings) {
        let table = &self.world.table;
        let owns = |code: &str| {
            table
                .code_owners
                .get(code)
                .is_some_and(|owner| (self.milestone.owns_code)(owner))
        };
        let hold = |code: &str| {
            self.milestone.host_rows_hold_gaps && self.host_code(table, code)
        };
        let mut owned_rows = 0usize;
        for (section, condition, codes) in
            chapter_rows(&self.world.chapter_07, &self.world.registry)
        {
            if codes.is_empty() || !codes.iter().all(|code| owns(code)) {
                continue;
            }
            owned_rows += 1;
            let label = format!("chapter 07, {section}: {condition}");
            if !codes.iter().any(|code| self.expected(code)) {
                if codes.iter().all(|code| hold(code)) {
                    findings.host.push(format!("{label}: the rejected side"));
                } else {
                    findings
                        .gaps
                        .push(format!("{label}: no case expects {codes:?}"));
                }
            }
            let citing: Vec<&OwnerRow> = table
                .rows
                .iter()
                .filter(|row| {
                    (self.milestone.owns_step)(&row.step)
                        && row.codes.iter().any(|code| codes.contains(code))
                })
                .collect();
            // A twin of a case that expects a code of the row, as
            // `matches_id` reads a twin, is a nearest-passing case of the row
            // too.
            let cases = &self.world.cases;
            let twin = cases
                .iter()
                .filter(|(_, case)| codes.iter().any(|code| self.expects(case, code)))
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
                        .flat_map(|pattern| self.matching(pattern))
                        .any(|id| cases.get(id).is_some_and(CaseFacts::accepted))
                });
            if !accepted {
                if self.milestone.host_rows_hold_gaps
                    && citing.iter().any(|row| row.names_host_tests())
                {
                    findings.host.push(format!("{label}: the accepted side"));
                } else {
                    findings.gaps.push(format!(
                        "{label}: no owner row of {codes:?} has an accepted case"
                    ));
                }
            }
        }
        findings.summary.push(format!(
            "chapter 07 rows whose codes {} owns: {owned_rows}",
            self.milestone.name
        ));
    }

    /// Every discharge rule has an accepted and a rejected case, and is
    /// recorded on a passed obligation of an accepted contract case.
    fn discharge_rules(self, findings: &mut Findings) {
        let rules = self.milestone.discharge_rules;
        if rules.is_empty() {
            return;
        }
        for rule in rules {
            let found = self.rule_cases(&self.world.table, rule);
            let side = |accepted: bool| {
                found
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.world
                            .cases
                            .get(*id)
                            .is_some_and(|case| case.accepted() == accepted)
                    })
                    .collect::<Vec<_>>()
            };
            let (accepted, rejected) = (side(true), side(false));
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
        // The first accepted contract case, checked again, whose passed
        // obligations record each rule.
        let mut first: BTreeMap<String, &str> = BTreeMap::new();
        for case in self.world.corpus.cases() {
            if rules.iter().all(|rule| first.contains_key(*rule)) {
                break;
            }
            let manifest = case.manifest();
            if !manifest.id.starts_with("V2-CONTRACT-")
                || manifest.expectations.verdict != Verdict::Accepted
            {
                continue;
            }
            let Some(source) = one_module(case) else {
                continue;
            };
            for obligation in check_source("main.vib", &source).obligations() {
                if obligation.status() == ObligationStatus::Passed {
                    for rule in obligation.rules() {
                        first.entry(rule.clone()).or_insert(&manifest.id);
                    }
                }
            }
        }
        for rule in rules {
            match first.get(*rule) {
                Some(id) => findings.summary.push(format!(
                    "{rule}: first recorded on a passed obligation of {id}"
                )),
                None => findings.gaps.push(format!(
                    "no passed obligation of an accepted contract case records {rule}"
                )),
            }
        }
    }

    /// No identity held by a host test is a case of the corpus.
    fn host_only(self, findings: &mut Findings) {
        for (id, ..) in self.milestone.host_only {
            if self.world.cases.contains_key(*id) {
                findings.gaps.push(format!(
                    "{id} is listed as held by a host test, but the corpus has the case"
                ));
            }
        }
    }

    /// Every row of the authority argument names identities, and each
    /// resolves.
    fn authority(self, findings: &mut Findings) {
        let Some((document, count)) = self.milestone.authority else {
            return;
        };
        let rows = authority_rows(&read(document));
        if rows.len() != count {
            findings.gaps.push(format!(
                "the authority argument has {} invariants, not {count}",
                rows.len()
            ));
        }
        for (number, ids) in &rows {
            if ids.is_empty() {
                findings.gaps.push(format!("{number}: no attack case"));
            }
            for id in ids {
                match self.resolves(id) {
                    Ok(how) => findings.summary.push(format!("{number} {id}: {how}")),
                    Err(gap) => findings.gaps.push(format!("{number}: {gap}")),
                }
            }
        }
    }

    /// One coverage map against its item of chapter 07: every item's words
    /// are in the item, in order, and every identity resolves.
    fn coverage(self, number: u32, map: CoverageMap, findings: &mut Findings) {
        let item = coverage_item(&self.world.chapter_07, number);
        if item.is_empty() {
            findings
                .gaps
                .push(format!("chapter 07 has no required coverage {number}"));
            return;
        }
        let mut at = 0usize;
        for (words, ids) in map {
            let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
            match item.get(at..).and_then(|rest| rest.find(&words)) {
                Some(offset) => at += offset + words.len(),
                None => findings.gaps.push(format!(
                    "coverage {number}: \"{words}\" is not the next item of the chapter"
                )),
            }
            if ids.is_empty() {
                findings
                    .gaps
                    .push(format!("coverage {number}: \"{words}\" has no case"));
            }
            let mut resolved = Vec::new();
            for id in *ids {
                match self.resolves(id) {
                    Ok(how) => resolved.push(format!("{id} ({how})")),
                    Err(gap) => findings
                        .gaps
                        .push(format!("coverage {number}: \"{words}\": {gap}")),
                }
            }
            findings
                .summary
                .push(format!("{number} \"{words}\": {}", resolved.join(", ")));
        }
    }

    /// The cases that expect `@tool.unavailable`, or are unsupported, are
    /// exactly the listed ones, and none holds a form of the milestone.
    fn unavailable(self, findings: &mut Findings) {
        let Some(list) = self.milestone.unavailable else {
            return;
        };
        let found: BTreeSet<&str> = self
            .world
            .cases
            .iter()
            .filter(|(_, case)| {
                self.expects(case, "@tool.unavailable")
                    || case.verdict == Verdict::Unsupported
            })
            .map(|(id, _)| id.as_str())
            .collect();
        let listed: BTreeSet<&str> = list.iter().map(|(id, ..)| *id).collect();
        for (id, form, owner) in list {
            findings.summary.push(format!("{id}: {form}; {owner}"));
            if owner.contains(self.milestone.name) {
                findings
                    .gaps
                    .push(format!("{id} is listed as a {} form", self.milestone.name));
            }
        }
        if found != listed {
            findings.gaps.push(format!(
                "the unavailable cases {found:?} are not the listed ones {listed:?}"
            ));
        }
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

/// The facts of every case of the corpus, by identity.
fn corpus_facts(
    corpus: &Corpus,
    registry: &BTreeSet<String>,
) -> BTreeMap<String, CaseFacts> {
    let mut facts = BTreeMap::new();
    for case in corpus.cases() {
        let manifest = case.manifest();
        let expect = &manifest.expectations;
        let mut codes: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
        for diagnostic in &expect.diagnostics {
            codes
                .entry(diagnostic.code.as_atom().to_owned())
                .or_default()
                .insert("diagnostics");
        }
        if let Some(event) = expect.host_event.as_ref() {
            codes
                .entry(event.as_atom().to_owned())
                .or_default()
                .insert("hostEvent");
        }
        for (key, document) in expect.snapshots() {
            let Some(key) = DOCUMENTS.iter().copied().find(|known| *known == key)
            else {
                continue;
            };
            let text = case
                .read_file(document)
                .expect("an expected document reads");
            for code in codes_in(&text, registry) {
                codes.entry(code).or_default().insert(key);
            }
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

/// The one module of a case: its `source`, or the only `.vib` file of its
/// tree's `src`.
fn one_module(case: &Case) -> Option<String> {
    if let Some(source) = case.manifest().inputs.source.as_deref() {
        return case.read_file(source).ok();
    }
    let tree = case.tree_directory().ok()??;
    let mut found = Vec::new();
    let mut pending = vec![tree.join("src")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "vib") {
                found.push(path);
            }
        }
    }
    match found.as_slice() {
        [only] => std::fs::read_to_string(Path::new(only)).ok(),
        _ => None,
    }
}

/// `true` when the pattern names `id`: every identity that begins with the
/// text before a trailing `*`; otherwise the identity itself and every
/// identity that extends it by a `-` part, as a twin extends the case it
/// varies (`V2-CONTRACT-int2-nearest-failing` of `V2-CONTRACT-int2`).
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

/// The functions of the workspace's crates: every `module::function` of a
/// `fn` or `pub fn` written in a file `module.rs`, or in a `mod module`
/// block, and every `fn`, `pub fn` or `pub(crate) fn` by name with the files
/// that define it.
fn workspace_functions() -> (BTreeSet<String>, BTreeMap<String, Vec<PathBuf>>) {
    let mut sources = Vec::new();
    rust_sources(&workspace_root().join("crates"), &mut sources);
    let mut tests = BTreeSet::new();
    let mut functions: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).expect("a Rust file reads");
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut module = stem.clone();
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
            let (rest, test) = match line
                .strip_prefix("fn ")
                .or_else(|| line.strip_prefix("pub fn "))
            {
                Some(rest) => (rest, true),
                None => match line.strip_prefix("pub(crate) fn ") {
                    Some(rest) => (rest, false),
                    None => continue,
                },
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if test {
                tests.insert(format!("{module}::{name}"));
                tests.insert(format!("{stem}::{name}"));
            }
            functions.entry(name).or_default().push(path.clone());
        }
    }
    (tests, functions)
}

/// The text of the function `name` in `path`: from its `fn` line to the
/// first line that closes it at the same indentation.
fn function_text(path: &Path, name: &str) -> String {
    let text = std::fs::read_to_string(path).expect("a Rust file reads");
    let mut lines = text.lines();
    let mut body = String::new();
    let mut indent = None;
    for line in lines.by_ref() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("fn {name}("))
            || trimmed.starts_with(&format!("pub fn {name}("))
        {
            indent = Some(line.len() - trimmed.len());
            body.push_str(line);
            body.push('\n');
            break;
        }
    }
    let Some(indent) = indent else {
        return body;
    };
    let closer = format!("{}}}", " ".repeat(indent));
    for line in lines {
        body.push_str(line);
        body.push('\n');
        if line == closer {
            break;
        }
    }
    body
}

/// The rows of the authority argument, section 2 of its document: the
/// invariant's number and the identities of its attack cases.
fn authority_rows(architecture: &str) -> Vec<(String, Vec<String>)> {
    let mut rows = Vec::new();
    let mut inside = false;
    for line in architecture.lines() {
        if line.starts_with("## 2.") {
            inside = true;
            continue;
        }
        if inside && line.starts_with('#') {
            break;
        }
        if !inside {
            continue;
        }
        let Some(cells) = cells(line) else { continue };
        if let [number, _, _, _, attacks, _] = cells.as_slice()
            && number.starts_with('A')
            && number.chars().skip(1).all(|c| c.is_ascii_digit())
        {
            let ids = code_spans(attacks)
                .into_iter()
                .filter(|span| span.starts_with("V2-") || span.starts_with("ATK-"))
                .map(str::to_owned)
                .collect();
            rows.push((number.clone(), ids));
        }
    }
    rows
}

/// The text of required coverage item `number` of chapter 07, on one line.
fn coverage_item(chapter: &str, number: u32) -> String {
    let start = format!("{number}. **");
    let mut text = String::new();
    let mut inside = false;
    for line in chapter.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&start) {
            inside = true;
        } else if inside
            && (trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed
                    .split_once(". **")
                    .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit())))
        {
            break;
        }
        if inside {
            text.push_str(trimmed);
            text.push(' ');
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
