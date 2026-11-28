//! The coverage audit of the M2 exit.
//!
//! An M2 form is an owner row whose step column names an M2 step. Beside
//! the owner table the audit reads the authority argument
//! (`docs/roadmap/v0.0.2/m2/architecture.md`, section 2) and required
//! coverage 10, 11 and 12 of chapter 07. A `V2-` identity is a corpus case;
//! where the specification gives a form no observation a case can hold, the
//! identity is a host test, and [`HOST_ONLY`] names it with the sentence that
//! makes it so; the same holds for the codes of [`HOST_CODES`]. Each entry is
//! checked: the host test exists, produces the code, and the corpus has no
//! case of that identity.

use super::{Audit, CoverageMap, Milestone, OwnerTable, World, assert_covered};
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
const COVERAGE_10: CoverageMap = &[
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
const COVERAGE_11: CoverageMap = &[
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
const COVERAGE_12: CoverageMap = &[
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

/// M2's data.
const M2: Milestone = Milestone {
    name: "M2",
    owns_step: |step| step.contains("M2"),
    owns_code: |owner| owner.starts_with("M2"),
    documents: &["types", "result", "testRun", "targetRun"],
    host_rows_hold_gaps: false,
    chapters: false,
    discharge_rules: &[],
    host_only: HOST_ONLY,
    host_codes: HOST_CODES,
    authority: Some(("docs/roadmap/v0.0.2/m2/architecture.md", 31)),
    coverage: &[(10, COVERAGE_10), (11, COVERAGE_11), (12, COVERAGE_12)],
    unavailable: Some(UNAVAILABLE),
};

#[test]
fn m2_coverage() {
    assert_covered(&M2);
}

/// A case identity added to an owner row of M2 that no case has is one gap,
/// and the only one.
#[test]
fn m2_owner_gap_detected() {
    let world = World::get();
    let audit = Audit {
        world,
        milestone: &M2,
    };
    let document = world.owners();
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
    let rows = |text: &str| audit.rows(&OwnerTable::parse(text, world.registry()));
    assert!(rows(document).gaps.is_empty());
    let findings = rows(&altered);
    assert_eq!(findings.gaps.len(), 1, "{:?}", findings.gaps);
    assert!(
        findings.gaps.iter().any(|gap| gap.contains(invented)),
        "{:?}",
        findings.gaps
    );
}
