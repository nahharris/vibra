//! M4 Step 20: the effect metadata of the query surfaces. The position
//! envelope reads the witness of each root from the one graph the checker
//! reported its diagnostics from. The enclosing, application, and target rows
//! of a position, and the index rows, are asserted by the `V1-TOOL-*` cases.

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{
    WorkspaceSnapshot,
    index::index,
    query::{
        EffectsFact, PositionExpansion, SemanticFactStatus, query_position_expanded,
    },
    semantic::{CheckStatus, check_all},
};

use crate::support::TempDir;

const OPS: &str = "(deffect write\n  visibility: @public\n  (defn text (target str) void\n    visibility: @public\n    (do)))\n\n(deffect audit\n  visibility: @public\n  (defn record (target str) void\n    visibility: @public\n    effects: (write)\n    (write.text target)))\n";

const AUDIT: &str = "@demo@0.1.0/ops.log.audit";
fn effects_at(
    snapshot: &WorkspaceSnapshot,
    source_id: &str,
    source: &str,
    needle: &str,
    expansion: PositionExpansion,
) -> (SemanticFactStatus, Option<EffectsFact>) {
    let offset = source.find(needle).expect("needle");
    let query = query_position_expanded(snapshot, source_id, offset, None, expansion)
        .expect("position query");
    let fact = query.effects();
    (fact.status(), fact.value().cloned())
}

fn roots(row: &vibra_workspace::query::EffectRowFact) -> Vec<&str> {
    row.roots().iter().map(String::as_str).collect()
}

/// A project with the one library target `app`.
const APP_LIBRARY: &str = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @lib root: \"src\")) dependencies: (dict))\n";

#[test]
fn the_witness_is_the_chain_the_checker_reports() {
    const HELPER: &str = "(deffect write\n  visibility: @public\n  (defn text (target str event str) void\n    visibility: @public\n    (do)))\n\n(deffect audit\n  visibility: @public\n  (defn record (target str event str) void\n    visibility: @public\n    effects: (write)\n    (write.text target event)))\n";
    // `handle-request` omits `write`, which the chapter's example reports.
    const MAIN_OMITS: &str = "(import helper @app.helper)\n\n(defn handle-request (target str summary str) void\n  effects: (helper.audit)\n  (helper.audit.record target summary))\n";
    let project = TempDir::project(
        "witness",
        APP_LIBRARY,
        &[("src/helper.vib", HELPER), ("src/main.vib", MAIN_OMITS)],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let checked = check_all(&snapshot);
    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    let diagnostic = checked
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::EffectOutsideCeiling)
        .expect("one missing root");
    // The checker's chain: the primary span, then each related span.
    let reported = std::iter::once((
        diagnostic.source_id().expect("source").to_owned(),
        diagnostic.primary_span(),
    ))
    .chain(diagnostic.related().iter().map(|related| {
        (
            related.source_id.clone().expect("related source"),
            related.span,
        )
    }))
    .collect::<Vec<_>>();

    let (status, value) = effects_at(
        &snapshot,
        "src/main.vib",
        MAIN_OMITS,
        "(helper.audit.record target summary)",
        PositionExpansion {
            effect_witnesses: true,
        },
    );
    assert_eq!(status, SemanticFactStatus::Exact);
    let value = value.expect("effects value");
    let enclosing = value.enclosing().expect("enclosing");
    // The body performs both roots although the ceiling writes only one.
    assert_eq!(roots(enclosing.ceiling()), ["@demo@0.1.0/app.helper.audit"]);
    assert_eq!(
        roots(enclosing.performed()),
        [
            "@demo@0.1.0/app.helper.audit",
            "@demo@0.1.0/app.helper.write"
        ]
    );
    let witnesses = value.witnesses().expect("expanded");
    assert_eq!(witnesses.len(), 2, "one record for each performed root");
    let write = witnesses
        .iter()
        .find(|witness| witness.root() == "@demo@0.1.0/app.helper.write")
        .expect("write witness");
    let hops = write
        .hops()
        .iter()
        .map(|hop| (hop.source_id().to_owned(), hop.span()))
        .collect::<Vec<_>>();
    assert_eq!(hops, reported, "the query repeats the checker's witness");
    assert_eq!(write.hops()[0].callee().kind(), "operation");
    assert_eq!(
        write.hops()[0].callee().identity(),
        Some("@demo@0.1.0/app.helper.audit.record")
    );
    assert_eq!(write.hops()[1].source_id(), "src/helper.vib");
    assert_eq!(write.hops()[1].start_position().line, 12);

    // A compact query returns the same metadata without the witnesses.
    let (_, compact) = effects_at(
        &snapshot,
        "src/main.vib",
        MAIN_OMITS,
        "(helper.audit.record target summary)",
        PositionExpansion::default(),
    );
    let compact = compact.expect("effects value");
    assert!(compact.witnesses().is_none());
    assert_eq!(compact.enclosing(), value.enclosing());
    assert_eq!(compact.application(), value.application());
    assert_eq!(compact.target(), value.target());
}

#[test]
fn a_target_that_sorts_first_owns_a_source_both_reach() {
    let project = TempDir::project(
        "two-targets",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @zeta kind: @bin root: \"src/zeta\" entry: @zeta.main.execute effects: (array @ops.log.audit @ops.log.write)) (record name: @alpha kind: @bin root: \"src/alpha\" entry: @alpha.main.execute effects: (array @ops.log.audit)) (record name: @ops kind: @lib root: \"src/ops\")) dependencies: (dict))\n",
        &[
            (
                "src/zeta/main.vib",
                "(import log @ops.log)\n\n(defn execute () void\n  effects: (log.audit log.write)\n  (log.audit.record \"z\"))\n",
            ),
            (
                "src/alpha/main.vib",
                "(import log @ops.log)\n\n(defn execute () void\n  effects: (log.audit)\n  (do))\n",
            ),
            ("src/ops/log.vib", OPS),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let (_, value) = effects_at(
        &snapshot,
        "src/ops/log.vib",
        OPS,
        "(write.text target)",
        PositionExpansion::default(),
    );
    let target = value
        .expect("effects value")
        .target()
        .cloned()
        .expect("target");
    assert_eq!(target.entry().canonical(), "@demo@0.1.0/alpha.main.execute");
    assert_eq!(roots(target.effects()), [AUDIT]);
}

#[test]
fn a_broken_neighbour_does_not_change_an_exact_row_elsewhere() {
    const GOOD: &str = "(deffect audit\n  visibility: @public\n  (defn record (target str) void\n    visibility: @public\n    (do)))\n\n(defn log-it (target str) void\n  visibility: @public\n  effects: (audit)\n  (audit.record target))\n";
    const BROKEN: &str = "(import good @app.good)\n\n(defn mistyped () i32 \"text\")\n\n(defn half-checked () void\n  effects: (good.audit)\n  (do\n    (good.audit.record \"x\")\n    (missing-name)))\n\n(defn after-broken () void\n  effects: (good.audit)\n  (half-checked))\n";
    let project = TempDir::project(
        "recovery",
        APP_LIBRARY,
        &[("src/good.vib", GOOD), ("src/broken.vib", BROKEN)],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    assert_eq!(check_all(&snapshot).status(), CheckStatus::Diagnostics);

    // The unrelated module keeps its exact fact.
    let (status, value) = effects_at(
        &snapshot,
        "src/good.vib",
        GOOD,
        "(audit.record target)",
        PositionExpansion::default(),
    );
    assert_eq!(status, SemanticFactStatus::Exact);
    let value = value.expect("effects value");
    assert_eq!(
        roots(value.enclosing().expect("enclosing").performed()),
        ["@demo@0.1.0/app.good.audit"]
    );
    assert_eq!(
        roots(value.application().expect("application")),
        ["@demo@0.1.0/app.good.audit"]
    );

    // A body that did not check has a lower bound, never an exact row.
    let (status, _) = effects_at(
        &snapshot,
        "src/broken.vib",
        BROKEN,
        "(good.audit.record \"x\")",
        PositionExpansion::default(),
    );
    assert_eq!(status, SemanticFactStatus::Recovered);
    let (status, _) = effects_at(
        &snapshot,
        "src/broken.vib",
        BROKEN,
        "(half-checked))",
        PositionExpansion::default(),
    );
    assert_eq!(status, SemanticFactStatus::Recovered);
    // A function whose body did not check is recovered wherever it is read.
    let (status, _) = effects_at(
        &snapshot,
        "src/broken.vib",
        BROKEN,
        "\"text\"",
        PositionExpansion::default(),
    );
    assert_eq!(status, SemanticFactStatus::Recovered);
}

#[test]
fn a_rejected_external_binding_is_kept_while_the_checked_facts_are_omitted() {
    let project = TempDir::project(
        "external",
        APP_LIBRARY,
        &[(
            "src/main.vib",
            "(deffect audit\n  (defn record (message str) void\n    external: @host\n    symbol: \"audit.record\"))\n\n(defn broken () i32 (missing-name))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let document = index(&snapshot, None).expect("index");
    let operation = document
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == "app.main.audit.record")
        .expect("operation record");
    let external = operation.external().expect("a syntactic fact survives");
    assert_eq!(external.provider(), "host");
    assert_eq!(external.symbol(), "audit.record");
    assert!(operation.facts().is_none(), "the binding did not check");
    let broken = document
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == "app.main.broken")
        .expect("function record");
    assert!(
        broken.facts().is_none(),
        "a body that did not check has no row"
    );
    assert!(broken.external().is_none());
}
