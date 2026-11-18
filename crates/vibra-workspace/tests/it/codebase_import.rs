//! Import preparation checks the entire package and never runs its tests.

use crate::support::TempDir;
use vibra_workspace::codebase::graph::{EdgeKind, EdgeTarget, ObjectKey, ObjectKind};
use vibra_workspace::codebase::import::prepare_import;

fn project() -> TempDir {
    let root = TempDir::new("codebase-import");
    root.write("project.vibon", "(record format: @project.v1 package: (record name: \"import-case\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))");
    root.write("src/app/main.vib", "(defn main () void void)\n");
    root
}

#[test]
fn preparing_an_import_checks_tests_without_running_a_failing_assertion() {
    let root = project();
    root.write("tests/checks.vib", "(import assert @std.assert)\n(test \"fails only if executed\" (assert.true false))\n");
    let prepared = prepare_import(root.path());
    assert!(
        prepared.is_ok(),
        "test bodies must be checked, never executed: {prepared:?}"
    );
    let prepared = prepared.expect("accepted checked import");
    let assertion = prepared
        .graph
        .objects
        .iter()
        .find(|object| object.key.canonical().contains("std.assert.true"))
        .expect("manifest-owned assertion object");
    assert_eq!(assertion.kind, ObjectKind::Defn);
    let assertion_component = prepared
        .graph
        .components
        .iter()
        .find(|component| component.address == assertion.component_address)
        .expect("assertion component");
    let assertion_payload = String::from_utf8_lossy(&assertion_component.payload);
    assert!(assertion_payload.contains("trusted-assertion"));
    assert!(assertion_payload.contains(
        "(record kind: @atom value: \"member\") (record kind: @string value: \"true\")"
    ));
    let test = prepared
        .graph
        .objects
        .iter()
        .find(|object| object.key.canonical().contains("fails only if executed"))
        .expect("test object");
    let boolean_type = prepared
        .graph
        .objects
        .iter()
        .find(|object| object.key.canonical().contains("std.bool.bool\""))
        .expect("nominal boolean role object");
    assert!(
        prepared.graph.edges.iter().any(|edge| {
            edge.owner == test.address
                && edge.kind == EdgeKind::TypeReference
                && edge.target == EdgeTarget::Object(boolean_type.address.clone())
        }),
        "primitive-backed boolean type pins its nominal role identity"
    );
}

#[test]
fn preparing_an_import_rejects_invalid_test_source_before_any_store_write() {
    let root = project();
    root.write(
        "tests/checks.vib",
        "(import assert @std.assert)\n(test \"invalid\" (assert.true 1u32))\n",
    );
    let error =
        prepare_import(root.path()).expect_err("invalid test source was admitted");
    assert!(error.source_rejected(), "{error:?}");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.source_id() == Some("tests/checks.vib")),
        "{error:?}"
    );
    assert!(!root.path().join(".vibra").exists());
}

#[test]
fn preparing_an_import_rejects_invalid_unreferenced_test_helpers_without_test_bodies() {
    let root = project();
    root.write("tests/helper.vib", "(defn wrong () u32 \"not a number\")\n");
    let error = prepare_import(root.path())
        .expect_err("helper-only test modules must be checked");
    assert!(error.source_rejected(), "{error:?}");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.source_id() == Some("tests/helper.vib")),
        "{error:?}"
    );
    assert!(!root.path().join(".vibra").exists());
}

#[test]
fn shadowing_is_rejected_before_encoding_instead_of_becoming_published_data() {
    let root = project();
    root.write("src/app/main.vib", "(defn repeat (value u32) u32 (let value 1u32) value)\n(defn main () void void)\n");
    let error =
        prepare_import(root.path()).expect_err("shadowing must remain rejected");
    assert!(error.source_rejected());
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code()
                == vibra_diagnostics::DiagnosticCode::NameRedeclaration)
    );
    assert!(!root.path().join(".vibra").exists());
}

#[test]
fn checker_deferred_constructor_names_are_checked_before_graph_projection() {
    let root = project();
    root.write(
        "src/app/main.vib",
        "(defn boolean-value () bool (bool.true))\n(defn main () void void)\n",
    );
    assert!(
        prepare_import(root.path()).is_ok(),
        "a checked deferred constructor must project from typed IR"
    );

    root.write(
        "src/app/main.vib",
        "(defn boolean-value () bool (bool.missing))\n(defn main () void void)\n",
    );
    let error = prepare_import(root.path())
        .expect_err("unknown constructor is rejected by checking");
    assert!(error.source_rejected(), "{error:?}");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.source_id() == Some("src/app/main.vib") }),
        "the checker diagnostic belongs to the source constructor"
    );
}

#[test]
fn global_initializers_emit_explicit_dependency_edges() {
    let root = project();
    root.write(
        "src/app/main.vib",
        "(def source i32 1i32)\n(def copy i32 source)\n(defn main () void void)\n",
    );
    let prepared =
        prepare_import(root.path()).expect("checked global initializer snapshot");
    let object = |suffix: &str| {
        prepared
            .graph
            .objects
            .iter()
            .find(|object| object.key.canonical().contains(suffix))
            .expect("declaration object")
    };
    let source = object(".source\"");
    let copy = object(".copy\"");
    assert!(prepared.graph.edges.iter().any(|edge| {
        edge.owner == copy.address
            && edge.kind == EdgeKind::ValueReference
            && edge.target == EdgeTarget::Object(source.address.clone())
    }));
    assert!(prepared.graph.edges.iter().any(|edge| {
        edge.owner == copy.address
            && edge.kind == EdgeKind::Initializer
            && edge.target == EdgeTarget::Object(source.address.clone())
    }));
}

#[test]
fn trivia_and_lexical_generic_alpha_renaming_preserve_checked_addresses() {
    let root = project();
    root.write("src/app/main.vib", "(defn identity (value t) t where: (t any) visibility: @public value)\n(defn main () void void)\n");
    let first = prepare_import(root.path()).expect("generic checked snapshot");
    root.write("src/app/main.vib", "; trivia is attached separately\n(defn identity (item element) element\n where: (element any) visibility: @public item)\n\n(defn main () void void)\n");
    let renamed = prepare_import(root.path()).expect("alpha-renamed checked snapshot");
    let addresses = |prepared: &vibra_workspace::codebase::import::PreparedImport| {
        prepared
            .graph
            .objects
            .iter()
            .map(|object| {
                (
                    object.key.clone(),
                    object.address.clone(),
                    object.component_address.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(addresses(&first), addresses(&renamed));
    assert_ne!(first.graph.sources, renamed.graph.sources);
    assert_ne!(first.graph.revision.address, renamed.graph.revision.address);
}

#[test]
fn changing_an_effect_operation_body_changes_its_owner_and_callers() {
    let root = project();
    let first_source = "(deffect audit (defn record () void visibility: @public void))\n(defn main () void void)\n(defn caller () void effects: (audit) (audit.record))\n";
    root.write("src/app/main.vib", first_source);
    let first = prepare_import(root.path()).expect("checked effect snapshot");
    root.write(
        "src/app/main.vib",
        &first_source.replace(
            "visibility: @public void",
            "visibility: @public (do 1u32 void)",
        ),
    );
    let changed = prepare_import(root.path()).expect("changed checked effect snapshot");
    let address = |prepared: &vibra_workspace::codebase::import::PreparedImport,
                   suffix: &str| {
        prepared
            .graph
            .objects
            .iter()
            .find(|object| {
                object
                    .key
                    .canonical()
                    .contains(&format!("/app.main.{suffix}\""))
            })
            .expect("local owner")
            .address
            .clone()
    };
    assert_ne!(address(&first, "audit"), address(&changed, "audit"));
    assert_ne!(address(&first, "caller"), address(&changed, "caller"));
    assert_eq!(address(&first, "main"), address(&changed, "main"));
}

#[test]
fn contract_callers_pin_the_selected_implementation_body() {
    let root = project();
    let source = "(defint measured (defn value (item self) u32))\n(deftype meter u32 (impl measured (defn value (item self) u32 (match item (meter amount) amount))))\n(defn caller () u32 (measured.value (meter 3u32)))\n(defn main () void void)\n";
    root.write("src/app/main.vib", source);
    let first = prepare_import(root.path()).expect("checked contract snapshot");
    assert!(
        first.graph.entities.iter().any(|mapping| {
            mapping.key.canonical().contains("app.main.measured.value")
                && mapping.part == vec!["value"]
        }),
        "abstract contract members map to their interface owner part"
    );
    assert!(
        first.graph.components.iter().any(|component| {
            component
                .payload
                .windows(b"abstract-function".len())
                .any(|window| window == b"abstract-function")
        }),
        "the checked signature is retained with an explicit absent body"
    );
    root.write(
        "src/app/main.vib",
        &source.replace(
            "(meter amount) amount",
            "(meter amount) (u32.add amount 1u32)",
        ),
    );
    let changed = prepare_import(root.path()).expect("changed implementation snapshot");
    let address = |prepared: &vibra_workspace::codebase::import::PreparedImport,
                   suffix: &str| {
        prepared
            .graph
            .objects
            .iter()
            .find(|object| {
                object
                    .key
                    .canonical()
                    .contains(&format!("/app.main.{suffix}\""))
            })
            .expect("local function")
            .address
            .clone()
    };
    assert_ne!(address(&first, "caller"), address(&changed, "caller"));
    assert_eq!(address(&first, "main"), address(&changed, "main"));
}

#[test]
fn generic_contract_dispatch_pins_only_implementations_eligible_for_receiver() {
    let root = project();
    let source = |alpha_body: &str, beta_body: &str| {
        format!(
            "(defint mapper (defn echo (item self value u) u where: (u any)))\n(deftype alpha i32 (impl mapper (defn echo (item self value u) u where: (u any) {alpha_body})))\n(deftype beta i32 (impl mapper (defn echo (item self value u) u where: (u any) {beta_body})))\n(defn caller () str (mapper.echo (alpha 1i32) \"x\"))\n(defn main () void void)\n"
        )
    };
    let first_source = source("value", "value");
    root.write("src/app/main.vib", &first_source);
    let first = prepare_import(root.path()).expect("checked generic contract snapshot");

    let beta_source = source("value", "(do value value)");
    root.write("src/app/main.vib", &beta_source);
    let beta_changed =
        prepare_import(root.path()).expect("changed unrelated implementation");

    let implementation_address =
        |prepared: &vibra_workspace::codebase::import::PreparedImport,
         receiver: &str| {
            prepared
                .graph
                .objects
                .iter()
                .find(|object| {
                    matches!(
                        &object.key,
                        ObjectKey::Implementation { receiver: key, .. }
                            if key.contains(receiver)
                    )
                })
                .expect("explicit implementation object")
                .address
                .clone()
        };
    let caller_address =
        |prepared: &vibra_workspace::codebase::import::PreparedImport| {
            prepared
                .graph
                .objects
                .iter()
                .find(|object| object.key.canonical().contains("/app.main.caller\""))
                .expect("caller")
                .address
                .clone()
        };
    assert_ne!(
        implementation_address(&first, "/app.main.beta"),
        implementation_address(&beta_changed, "/app.main.beta")
    );
    assert_eq!(caller_address(&first), caller_address(&beta_changed));

    let alpha_source = source("(do value value)", "value");
    root.write("src/app/main.vib", &alpha_source);
    let alpha_changed =
        prepare_import(root.path()).expect("changed eligible implementation");
    assert_ne!(
        implementation_address(&first, "/app.main.alpha"),
        implementation_address(&alpha_changed, "/app.main.alpha")
    );
    assert_ne!(caller_address(&first), caller_address(&alpha_changed));
}
