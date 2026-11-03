//! Step 6 binding contract tests that a corpus case cannot state: the typed
//! observation of a binding case is valid VIBON, and a global initializer keeps
//! its local slots at runtime.
//!
//! The results of the rest of the contract are corpus cases:
//! `V1-RUNTIME-bindings`, `V1-TYPE-NAMES-binding-cycle` (exactly one
//! `@type.initializer-cycle`), `V1-TYPE-NAMES-binding-discards`, and
//! `V1-TYPE-NAMES-binding-constant-only`.

use crate::support::corpus_case;

use std::path::Path;

use vibra_conformance::{ProfileHandler, StaticV1TypeHandler};
use vibra_syntax::parse_data;

#[test]
fn checked_binding_observation_is_valid_vibon() {
    let case = corpus_case("V1-RUNTIME-bindings");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    let types = observation.types.expect("typed program observation");
    let document = parse_data(Path::new("types.vibon"), &types)
        .expect("typed binding observation loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
}

#[test]
fn global_initializer_keeps_its_local_slots_at_runtime() {
    let checked = vibra_types::check_source(
        "global-let.vib",
        "(def value i32 (do (let local 1i32) local))\n(defn answer () i32 value)",
    );
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("checked program");
    assert_eq!(program.globals()[0].slot_count(), 1);
    let execution = vibra_interp::run(program).expect("global initializer");
    assert_eq!(
        execution.value(),
        Some(&vibra_ir::Value::I32(1)),
        "global local binding must evaluate through the checked IR"
    );
}
