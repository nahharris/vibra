//! The support inventory of the WebAssembly transition profile:
//! `conformance/wasm-support.tsv` has one row per executable case of the
//! corpus, `id<TAB>support<TAB>node`, written by hand from the runner's
//! report. This test classifies every executable case again and compares,
//! so a case that moves from `supported` to `unsupported`, or a node that
//! changes, fails here.

use std::collections::BTreeMap;

use vibra_conformance::{
    Corpus, DispatchResult, WASM_SUPPORT_HEADER, corpus_root, standard_dispatcher,
    unsupported_node, workspace_root,
};

/// The rows of the inventory, by case identity: the support and the node.
fn inventory() -> BTreeMap<String, (String, String)> {
    let path = workspace_root()
        .join("conformance")
        .join("wasm-support.tsv");
    let text = std::fs::read_to_string(&path).expect("the inventory is readable");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(WASM_SUPPORT_HEADER), "the header");
    let mut rows = BTreeMap::new();
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        let [id, support, node] = fields.as_slice() else {
            panic!("a row has three fields: {line:?}");
        };
        match *support {
            "supported" => {
                assert!(node.is_empty(), "a supported row names no node: {id}")
            }
            "unsupported" => {
                assert!(!node.is_empty(), "an unsupported row names its node: {id}")
            }
            other => panic!("`{other}` is not a support value ({id})"),
        }
        let previous = rows.insert(
            (*id).to_owned(),
            ((*support).to_owned(), (*node).to_owned()),
        );
        assert!(previous.is_none(), "{id} has one row");
    }
    rows
}

/// The runner's classification of every executable case whose handler
/// built checked IR.
fn classification() -> BTreeMap<String, (String, String)> {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    let dispatcher = standard_dispatcher();
    let mut rows = BTreeMap::new();
    for case in corpus.cases() {
        if !case.manifest().operation.executes_programs() {
            continue;
        }
        let DispatchResult::Executed { observation, .. } = dispatcher.dispatch(case)
        else {
            continue;
        };
        let Some(executable) = &observation.executable else {
            continue;
        };
        let row = match unsupported_node(executable).expect("the emitter has no defect")
        {
            None => ("supported".to_owned(), String::new()),
            Some(node) => ("unsupported".to_owned(), node.to_owned()),
        };
        rows.insert(case.manifest().id.clone(), row);
    }
    rows
}

#[test]
fn wasm_support_inventory() {
    let inventory = inventory();
    assert_eq!(inventory, classification());
    let text = std::fs::read(workspace_root().join("conformance/wasm-support.tsv"))
        .expect("readable");
    assert!(
        text.ends_with(b"\n") && !text.contains(&b'\r'),
        "LF line endings"
    );
}
