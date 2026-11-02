//! Effect witnesses on a call chain too deep to state as a case
//! (`docs/spec/03-effects.md`; M4 Step 14). Effect declarations, rows,
//! performed rows, and the witness of `@effect.outside-ceiling` are asserted
//! by the `V1-EFFECT-*` conformance cases.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_types::check_source;

#[test]
fn a_deep_call_chain_is_searched_without_host_recursion() {
    // 3000 functions each calling the next; the last performs `write`. The
    // outermost, with an empty ceiling, is the only offender.
    let depth = 3000;
    let mut source = String::from(
        "(deffect write (defn text (target str) void (do)))\n(defn hop0 () void effects: (write) (write.text \"x\"))\n",
    );
    for index in 1..depth {
        source.push_str(&format!(
            "(defn hop{index} () void effects: (write) (hop{}))\n",
            index - 1
        ));
    }
    source.push_str(&format!("(defn top () void (hop{}))\n", depth - 1));
    let checked = check_source("effects.vib", &source);
    let found = checked.diagnostics();
    assert_eq!(found.len(), 1, "{}", found.len());
    assert_eq!(
        found[0].related().len(),
        usize::try_from(depth).expect("depth")
    );
}
