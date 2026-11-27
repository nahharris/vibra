//! The coverage audit of the M2 exit.
//!
//! The tests read the owner table (`docs/roadmap/v0.0.2/m0/owners.md`), the
//! authority argument (`docs/roadmap/v0.0.2/m2/architecture.md`, section 2),
//! required coverage 10, 11 and 12 of chapter 07, and the committed corpus,
//! and fail on every gap at once:
//!
//! - an owner row of an M2 form that names a case identity the corpus does
//!   not have, an `ATK-` test or a `module::function` test the workspace does
//!   not define, no accepted case, or no rejected case where the row names a
//!   diagnostic;
//! - a code the owner table gives to M2 that no case expects;
//! - an identity of the authority argument that does not resolve;
//! - an item of required coverage 10, 11 or 12 with no case or test;
//! - a case that expects `@tool.unavailable` for an M2 form.
//!
//! A `V2-` identity is a corpus case. Where the specification gives a form
//! no observation a case can hold, the identity is a host test, and
//! [`HOST_ONLY`] names it with the sentence that makes it so; the same holds
//! for the codes of [`HOST_CODES`]. Each entry is checked: the host test
//! exists and the corpus has no case of that identity.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use vibra_conformance::{Corpus, Verdict, corpus_root, workspace_root};

use super::owner_coverage::{
    cells, code_spans, codes_in, host_test_names, is_separator, is_test_path,
    matches_id, read, registry,
};

/// A `V2-` identity that is a host test because no case can observe it:
/// the identity, the host test, and why.
const HOST_ONLY: &[(&str, &str, &str)] = &[
    (
        "V2-GRANT-startup-failed-optional",
        "v2_grant_startup_failed_optional",
        "06 Host world, \"Recipes under a world\": every recipe resolves against \
         the world, whose directories, streams, variables, arguments, clocks and \
         random source always exist, so none of the failures of 04 \"Startup order\" \
         has a world form",
    ),
    (
        "V2-GRANT-release-order",
        "v2_grant_release_order",
        "06 Entry outcome: release runs no program code and appends no event; the \
         startup record states each binding's status, not the order of release, \
         which only the driver observes",
    ),
];

/// A code the owner table gives to M2 that no case can expect: the code,
/// the host test that produces it, the text by which that test asserts it,
/// and why.
const HOST_CODES: &[(&str, &str, &str, &str)] = &[
    (
        "@runtime.invalidHostValue",
        "atk_forged_token",
        // `invalid` of `vibra-host/tests/it/broker.rs` matches
        // `HostFault::InvalidHostValue`.
        "assert!(invalid(",
        "06 Host calls: an invalid, stale, or foreign boundary value; checked \
         source cannot make one, so only a host test hands the broker one",
    ),
    (
        "@grant.acquisitionFailed",
        "v2_grant_startup_failed_optional",
        "FailureCode::AcquisitionFailed",
        "04 Startup order, the Failed column, has no world form (see \
         `V2-GRANT-startup-failed-optional`)",
    ),
];

/// The `@tool.unavailable` cases of the corpus: the case, the form it
/// holds, and that form's owner. None is an M2 form.
const UNAVAILABLE: &[(&str, &str, &str)] = &[
    (
        "V2-PROJECT-git-dependency-unavailable",
        "a `@git` dependency",
        "not in this release (owner table, Findings)",
    ),
    (
        "V2-PROJECT-test-unavailable-closure",
        "a contract call on an interface's type parameter whose bound leaves an \
         associated type open",
        "the checker of M1 (an interface takes no extra parameter for the choice)",
    ),
];

/// Required coverage 10 of chapter 07, item by item: the item's words and
/// the cases or host tests that hold it.
const COVERAGE_10: &[(&str, &[&str])] = &[
    (
        "Complete rows through callbacks, records, closures, and plans",
        &[
            "V2-EFFECT-outside-ceiling-callback",
            "V2-EFFECT-callback-row-wider",
            "V2-EFFECT-proc-type-field",
            "V2-EFFECT-field-row-mismatch",
            "V2-EFFECT-anon-proc-outside-ceiling",
            "V2-RUNTIME-anon-proc-captures",
            "V2-EFFECT-outside-ceiling-stored-plan",
            "V2-EFFECT-proc-plan-bound",
        ],
    ),
    (
        "owner plus additive row on interface members",
        &[
            "V2-EFFECT-member-additive-row",
            "V2-EFFECT-member-owner-in-row",
        ],
    ),
    ("no `await` in a `fn`", &["V2-EFFECT-await-in-fn"]),
    (
        "building against running a plan",
        &["V2-RUNTIME-plan-built-not-run", "V2-EFFECT-plan-discarded"],
    ),
    (
        "guards with `else` run on each `await`",
        &[
            "V2-RUNTIME-requires-else-on-await",
            "V2-RUNTIME-ensures-else-on-await",
        ],
    ),
    (
        "each `await` is a new invocation, with no implicit retry",
        &[
            "V2-RUNTIME-plan-awaited-twice",
            "V2-RUNTIME-await-twice-two-events",
            "ATK-no-redispatch-on-error",
        ],
    ),
    (
        "wrong, omitted, and duplicate rows",
        &[
            "V2-EFFECT-row-names-type",
            "V2-EFFECT-std-row-missing",
            "V2-EFFECT-row-duplicate",
        ],
    ),
    (
        "shortest witnesses",
        &[
            "V2-EFFECT-outside-ceiling-transitive",
            "V2-EFFECT-outside-ceiling-cycle",
        ],
    ),
    (
        "required and optional grants",
        &[
            "V2-GRANT-required-optional-bundle",
            "V2-RUNTIME-run-optional-absent",
            "V2-RUNTIME-run-required-refused",
        ],
    ),
    ("brands", &["V2-GRANT-brand", "V2-GRANT-brand-forged"]),
    (
        "forged, stale, and foreign grants",
        &[
            "V2-GRANT-no-constructor",
            "ATK-forged-token",
            "ATK-stale-grant",
            "ATK-foreign-grant",
        ],
    ),
    (
        "narrowing against interface views",
        &["V2-GRANT-view-does-not-narrow"],
    ),
    ("limits never reset", &["V2-GRANT-limit-child-after-use"]),
];

/// Required coverage 11 of chapter 07, without the reserved HTTP rules.
const COVERAGE_11: &[(&str, &[&str])] = &[
    (
        "Whole rules stay correlated",
        &[
            "V2-GRANT-rule-correlation",
            "ATK-policy-rule-fields-crossed",
        ],
    ),
    (
        "restriction keeps the parent",
        &[
            "V2-GRANT-restrict-keeps-parent",
            "V2-GRANT-parent-stays-valid",
        ],
    ),
    (
        "normalization and size limits",
        &[
            "V2-GRANT-policy-4096-nodes",
            "V2-GRANT-policy-4097-nodes",
            "V2-GRANT-policy-depth-64",
            "V2-GRANT-policy-depth-65",
            "V2-GRANT-restrict-too-complex",
        ],
    ),
    (
        "a read-only directory chosen at run time with a `.txt` rule",
        &["V2-GRANT-release-optional-absent"],
    ),
    (
        "the real name of an entry",
        &[
            "ATK-fs-alias-name",
            "ATK-fs-case-alias",
            "ATK-run-alias-denied",
        ],
    ),
    (
        "a rejected `RelativePath` is `ScopeError.invalidPath()`",
        &["V2-RUNTIME-relative-path-invalid-path"],
    ),
    (
        "prefix by names, not by string",
        &["V2-GRANT-prefix-by-names", "ATK-run-prefix-by-names"],
    ),
    (
        "reparse points, links, mounts, renames, and a replaced root",
        &[
            "ATK-fs-junction",
            "ATK-fs-file-symlink",
            "ATK-run-symlink-denied",
            "ATK-run-junction-denied",
            "ATK-run-unc-root",
            "ATK-fs-root-renamed",
            "ATK-run-root-replaced",
        ],
    ),
    (
        "no cached intermediate handle",
        &["ATK-fs-swapped-junction", "ATK-run-swapped-component"],
    ),
    (
        "hard-link and opened-object limits shown at approval",
        &[
            "ATK-fs-hard-link-replace",
            "ATK-run-hard-link-replace-denied",
            "ATK-run-hard-link-read-allowed",
            "ATK-gate-replace-without-link-count",
        ],
    ),
    (
        "shared counters under siblings",
        &["V2-GRANT-limit-siblings-share", "V2-GRANT-copy-shares"],
    ),
    (
        "Passing in the fake world does not qualify a real adapter",
        &[
            "ATK-run-denied-leaves-file",
            "ATK-run-create-denied-leaves-directory",
        ],
    ),
];

/// Required coverage 12 of chapter 07.
const COVERAGE_12: &[(&str, &[&str])] = &[
    (
        "Every operation, with success and each typed error, in fresh worlds",
        &[
            "V2-EFFECT-std-each-interface",
            "V2-GRANT-test-each-fs-operation",
            "V2-GRANT-test-fs-errors",
            "V2-GRANT-test-stream-bytes",
            "V2-GRANT-test-clock-overflow",
            "V2-GRANT-denied-is-value",
            "V2-GRANT-limit-exceeded-is-value",
            "V2-GRANT-test-worlds-isolated",
            "V2-RUNTIME-run-fresh-world",
        ],
    ),
    ("standard input read twice", &["V2-GRANT-test-stdin-twice"]),
    (
        "output captured under test",
        &[
            "V2-GRANT-test-output-captured",
            "V2-GRANT-test-output-two-stream-sets",
        ],
    ),
    (
        "sorted directory entries",
        &[
            "V2-GRANT-test-entries-sorted",
            "V2-GRANT-test-directory-two-files",
        ],
    ),
    ("create and replace", &["V2-GRANT-test-create-and-replace"]),
    (
        "invalid paths and text",
        &[
            "V2-RUNTIME-relative-path-invalid-path",
            "V2-GRANT-test-fs-errors",
        ],
    ),
    (
        "present and absent variables",
        &["V2-GRANT-test-variables", "V2-GRANT-env-test-any-name"],
    ),
    ("exact arguments", &["V2-GRANT-test-arguments"]),
    (
        "fake clock steps and random vectors",
        &["V2-GRANT-test-clock-steps", "V2-GRANT-test-random-vector"],
    ),
    (
        "Events are recorded once, in order",
        &["V2-RUNTIME-trace-order"],
    ),
    (
        "a trap or exhaustion invents no event",
        &[
            "V2-RUNTIME-trace-trap-no-event",
            "ATK-trap-on-each-side-of-the-commit",
        ],
    ),
    (
        "projections restore no authority",
        &["ATK-trace-decode-grants-nothing"],
    ),
    (
        "The startup order on a permuted `grants` dict in which acquiring the \
         first ready leaf makes a lexically earlier leaf ready",
        &["V2-GRANT-startup-order-permuted"],
    ),
    (
        "startup failure",
        &[
            "V2-RUNTIME-run-required-refused",
            "V2-GRANT-startup-failed-optional",
        ],
    ),
    (
        "optional absence, including an absent `source` leaf",
        &[
            "V2-RUNTIME-run-optional-absent",
            "V2-GRANT-release-optional-absent",
        ],
    ),
    ("release order", &["V2-GRANT-release-order"]),
    (
        "non-zero exit codes stay distinct from tool exits and traps",
        &[
            "V2-RUNTIME-run-nonzero",
            "V2-RUNTIME-run-code-255",
            "V2-RUNTIME-run-exhausted",
        ],
    ),
];

/// One case of the corpus, as the audit reads it.
#[derive(Debug)]
struct Facts {
    verdict: Verdict,
    /// The codes of its diagnostics, its host event, and every code its
    /// expected documents write.
    codes: BTreeSet<String>,
}

impl Facts {
    fn accepted(&self) -> bool {
        self.verdict == Verdict::Accepted
    }

    fn rejected(&self) -> bool {
        self.verdict != Verdict::Accepted && !self.codes.is_empty()
    }
}

/// What the audits read: the registry, the corpus, and the workspace's
/// functions.
struct World {
    registry: BTreeSet<String>,
    cases: BTreeMap<String, Facts>,
    /// Every `fn` of the workspace's crates, by name, with its file.
    functions: BTreeMap<String, Vec<PathBuf>>,
    /// Every `module::function` of the workspace's crates.
    tests: BTreeSet<String>,
}

impl World {
    fn load() -> Self {
        let chapter_07 = read("docs/spec/07-diagnostics-and-conformance.md");
        let registry = registry(&chapter_07);
        let cases = corpus_facts(&registry);
        Self {
            registry,
            cases,
            functions: functions(),
            tests: host_test_names(),
        }
    }

    /// The cases a pattern names, as the owner audit of M1 reads one.
    fn matching(&self, pattern: &str) -> Vec<&str> {
        self.cases
            .keys()
            .filter(|id| matches_id(pattern, id))
            .map(String::as_str)
            .collect()
    }

    /// The host tests an `ATK-` identity or pattern names: `ATK-forged-token`
    /// is `atk_forged_token` or a test whose name extends it by `_`, and
    /// `ATK-fs-*` every test whose name begins with `atk_fs_`.
    fn attack_tests(&self, id: &str) -> Vec<String> {
        let stem = id
            .trim_start_matches("ATK-")
            .trim_end_matches('*')
            .replace('-', "_");
        let name = format!("atk_{stem}");
        self.functions
            .keys()
            .filter(|function| {
                if id.ends_with('*') {
                    function.starts_with(&name)
                } else {
                    **function == name || function.starts_with(&format!("{name}_"))
                }
            })
            .cloned()
            .collect()
    }

    /// Whether an identity of a coverage map or the authority argument
    /// resolves: a `V2-` case of the corpus, a `V2-` identity of
    /// [`HOST_ONLY`] whose host test exists, or an `ATK-` host test.
    fn resolves(&self, id: &str) -> Result<String, String> {
        if id.starts_with("ATK-") {
            let found = self.attack_tests(id);
            return if found.is_empty() {
                Err(format!("no host test `{id}`"))
            } else {
                Ok(format!("host test {}", found.join(", ")))
            };
        }
        if self.cases.contains_key(id) {
            return Ok("case".to_owned());
        }
        match HOST_ONLY.iter().find(|(only, ..)| *only == id) {
            Some((_, test, _)) if self.functions.contains_key(*test) => {
                Ok(format!("host test {test}"))
            }
            Some((_, test, _)) => Err(format!("`{id}`: no host test `{test}`")),
            None => Err(format!("no case `{id}`")),
        }
    }
}

/// Every `fn` of the workspace's crates, with the files that define it.
fn functions() -> BTreeMap<String, Vec<PathBuf>> {
    let mut found: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
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
            for line in text.lines() {
                let line = line.trim_start();
                let Some(rest) = line
                    .strip_prefix("fn ")
                    .or_else(|| line.strip_prefix("pub fn "))
                    .or_else(|| line.strip_prefix("pub(crate) fn "))
                else {
                    continue;
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                found.entry(name).or_default().push(path.clone());
            }
        }
    }
    found
}

/// The text of the function `name` in `path`: from its `fn` line to the
/// first line that closes it at the same indentation.
fn function_text(path: &PathBuf, name: &str) -> String {
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

/// The facts of every case of the committed corpus, by identity.
fn corpus_facts(registry: &BTreeSet<String>) -> BTreeMap<String, Facts> {
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
        for document in [
            &expect.types,
            &expect.result,
            &expect.test_run,
            &expect.target_run,
        ]
        .into_iter()
        .flatten()
        {
            let text = case
                .read_file(document)
                .expect("an expected document reads");
            codes.extend(codes_in(&text, registry));
        }
        facts.insert(
            manifest.id.clone(),
            Facts {
                verdict: expect.verdict,
                codes,
            },
        );
    }
    facts
}

/// What an audit found.
#[derive(Default, Debug)]
struct Findings {
    gaps: Vec<String>,
    /// What host tests hold, as the row says.
    host: Vec<String>,
    summary: Vec<String>,
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

    fn assert_no_gap(&self) {
        assert!(
            self.gaps.is_empty(),
            "{} gaps:\n{}",
            self.gaps.len(),
            self.gaps.join("\n")
        );
    }
}

/// An owner row is an M2 form when its step column names an M2 step.
fn is_m2(step: &str) -> bool {
    step.contains("M2")
}

/// The owner audit over `document`, an owner table.
fn owner_audit(document: &str, world: &World) -> Findings {
    let mut findings = Findings::default();
    let mut section = String::new();
    let mut header_seen = false;
    let mut checked = 0usize;
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
        if is_separator(&cells)
            || section == "Diagnostic codes"
            || section == "Findings"
        {
            continue;
        }
        let [form, step, cases, diagnostics, ..] = cells.as_slice() else {
            continue;
        };
        if !is_m2(step) {
            continue;
        }
        checked += 1;
        let label = format!("{section} / {form}");
        let spans = code_spans(cases);
        let patterns: Vec<&str> = spans
            .iter()
            .copied()
            .filter(|span| span.starts_with("V1-") || span.starts_with("V2-"))
            .collect();
        let attacks: Vec<&str> = spans
            .iter()
            .copied()
            .filter(|span| span.starts_with("ATK-"))
            .collect();
        let paths: Vec<&str> = spans
            .iter()
            .copied()
            .filter(|span| is_test_path(span))
            .collect();
        let host = cases.contains("host test");
        let codes = codes_in(diagnostics, &world.registry);

        for attack in &attacks {
            if world.attack_tests(attack).is_empty() {
                findings.gaps.push(format!(
                    "{label}: no host test `{attack}`, which the row names"
                ));
            }
        }
        for path in &paths {
            if !world.tests.contains(*path) {
                findings.gaps.push(format!(
                    "{label}: no host test `{path}`, which the row names"
                ));
            }
        }
        if patterns.is_empty() {
            if attacks.is_empty() && paths.is_empty() && !host {
                findings
                    .gaps
                    .push(format!("{label}: names no case and no host test"));
            } else {
                findings.host.push(format!("{label}: every side"));
            }
            continue;
        }
        let mut named: BTreeSet<&str> = BTreeSet::new();
        for pattern in &patterns {
            let found = world.matching(pattern);
            if found.is_empty() {
                findings.gaps.push(format!("{label}: no case `{pattern}`"));
            }
            named.extend(found);
        }
        let facts: Vec<&Facts> =
            named.iter().filter_map(|id| world.cases.get(*id)).collect();
        let mut side = |missing: bool, what: &str| {
            if !missing {
                return;
            }
            if host || !attacks.is_empty() {
                findings.host.push(format!("{label}: the {what} side"));
            } else {
                findings.gaps.push(format!("{label}: no {what} case"));
            }
        };
        side(!facts.iter().any(|case| case.accepted()), "accepted");
        side(
            !codes.is_empty() && !facts.iter().any(|case| case.rejected()),
            "rejected",
        );
    }
    findings
        .summary
        .push(format!("owner rows of M2 forms checked: {checked}"));
    findings
}

/// The codes the owner table gives to M2: those whose owner cell names M2.
fn m2_codes(document: &str) -> Vec<String> {
    let mut codes = Vec::new();
    let mut in_codes = false;
    for line in document.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            in_codes = heading.trim() == "Diagnostic codes";
            continue;
        }
        if !in_codes {
            continue;
        }
        if let Some(cells) = cells(line)
            && let [code, owner] = cells.as_slice()
            && code.starts_with("`@")
            && owner.starts_with("M2")
        {
            codes.push(code.trim_matches('`').to_owned());
        }
    }
    codes
}

/// The rows of the authority argument: the invariant's number and the
/// identities of its attack cases.
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
fn coverage_item(number: u32) -> String {
    let chapter = read("docs/spec/07-diagnostics-and-conformance.md");
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

/// Checks one coverage map against its item of chapter 07: every item's
/// words are in the item, in order, and every identity resolves.
fn coverage(number: u32, map: &[(&str, &[&str])], world: &World) -> Findings {
    let item = coverage_item(number);
    assert!(
        !item.is_empty(),
        "chapter 07 has required coverage {number}"
    );
    let mut findings = Findings::default();
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
            match world.resolves(id) {
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
    findings
}

#[test]
fn m2_owner_forms_have_cases() {
    let world = World::load();
    let findings = owner_audit(&read("docs/roadmap/v0.0.2/m0/owners.md"), &world);
    findings.print();
    findings.assert_no_gap();
}

#[test]
fn m2_owner_gap_detected() {
    let world = World::load();
    let document = read("docs/roadmap/v0.0.2/m0/owners.md");
    let invented = "V2-EFFECT-invented-for-the-audit";
    let altered = document.replacen(
        "`V2-EFFECT-proc-type-*`, `V2-EFFECT-intent-*`",
        &format!("`V2-EFFECT-proc-type-*`, `{invented}`, `V2-EFFECT-intent-*`"),
        1,
    );
    assert_ne!(
        altered, document,
        "the owner table names `V2-EFFECT-intent-*`"
    );
    assert!(owner_audit(&document, &world).gaps.is_empty());
    let findings = owner_audit(&altered, &world);
    assert_eq!(findings.gaps.len(), 1, "{:?}", findings.gaps);
    assert!(
        findings.gaps.iter().any(|gap| gap.contains(invented)),
        "{:?}",
        findings.gaps
    );
}

#[test]
fn m2_diagnostics_have_cases() {
    let world = World::load();
    let codes = m2_codes(&read("docs/roadmap/v0.0.2/m0/owners.md"));
    assert!(!codes.is_empty(), "the owner table gives codes to M2");
    let mut gaps = Vec::new();
    for code in &codes {
        assert!(world.registry.contains(code), "{code} is a registry code");
        let cases: Vec<&str> = world
            .cases
            .iter()
            .filter(|(_, case)| case.codes.contains(code))
            .map(|(id, _)| id.as_str())
            .collect();
        let host = HOST_CODES.iter().find(|(held, ..)| held == code);
        match (cases.first(), host) {
            (Some(first), None) => println!("{code}: {first} and {} more", cases.len() - 1),
            (None, Some((_, test, needle, reason))) => {
                let Some(paths) = world.functions.get(*test) else {
                    gaps.push(format!("{code}: no host test `{test}`"));
                    continue;
                };
                let asserts = paths
                    .iter()
                    .any(|path| function_text(path, test).contains(needle));
                if asserts {
                    println!("{code}: host test {test}: {reason}");
                } else {
                    gaps.push(format!("{code}: host test `{test}` does not produce it"));
                }
            }
            (Some(first), Some(_)) => gaps.push(format!(
                "{code} is in HOST_CODES, but the case {first} expects it: remove the entry"
            )),
            (None, None) => gaps.push(format!("the M2 code {code} is expected by no case")),
        }
    }
    println!("codes the owner table gives to M2: {}", codes.len());
    assert!(gaps.is_empty(), "{} gaps:\n{}", gaps.len(), gaps.join("\n"));
}

#[test]
fn m2_authority_rows_have_cases() {
    let world = World::load();
    let rows = authority_rows(&read("docs/roadmap/v0.0.2/m2/architecture.md"));
    assert_eq!(rows.len(), 31, "the authority argument has 31 invariants");
    let mut gaps = Vec::new();
    for (number, ids) in &rows {
        if ids.is_empty() {
            gaps.push(format!("{number}: no attack case"));
        }
        for id in ids {
            match world.resolves(id) {
                Ok(how) => println!("{number} {id}: {how}"),
                Err(gap) => gaps.push(format!("{number}: {gap}")),
            }
        }
    }
    for (id, ..) in HOST_ONLY {
        if world.cases.contains_key(*id) {
            gaps.push(format!("{id} is in HOST_ONLY, but the corpus has the case"));
        }
    }
    assert!(gaps.is_empty(), "{} gaps:\n{}", gaps.len(), gaps.join("\n"));
}

#[test]
fn m2_coverage_10() {
    let world = World::load();
    let findings = coverage(10, COVERAGE_10, &world);
    findings.print();
    findings.assert_no_gap();
}

#[test]
fn m2_coverage_11() {
    let world = World::load();
    let findings = coverage(11, COVERAGE_11, &world);
    findings.print();
    findings.assert_no_gap();
}

#[test]
fn m2_coverage_12() {
    let world = World::load();
    let findings = coverage(12, COVERAGE_12, &world);
    findings.print();
    findings.assert_no_gap();
}

#[test]
fn m2_no_unavailable_m2_form() {
    let world = World::load();
    let unavailable: BTreeSet<&str> = world
        .cases
        .iter()
        .filter(|(_, case)| {
            case.codes.contains("@tool.unavailable")
                || case.verdict == Verdict::Unsupported
        })
        .map(|(id, _)| id.as_str())
        .collect();
    let listed: BTreeSet<&str> = UNAVAILABLE.iter().map(|(id, ..)| *id).collect();
    for (id, form, owner) in UNAVAILABLE {
        println!("{id}: {form}; {owner}");
        assert!(!owner.contains("M2"), "{id} is listed as an M2 form");
    }
    assert_eq!(
        unavailable, listed,
        "every unavailable case is listed with its form and owner, and none is an M2 form"
    );
}
