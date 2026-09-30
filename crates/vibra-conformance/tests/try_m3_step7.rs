//! M3 Step 7: `try` exits a tail-recursive loop without growing activation
//! depth, keeping the Step 9 M2 tail-call guarantee.

#![allow(clippy::expect_used)]

use vibra_types::check_source;

#[test]
fn a_tail_recursive_loop_exits_through_try_at_constant_depth() {
    let run = |count: usize| {
        let items = vec!["1i32"; count].join(" ");
        let source = format!(
            "\
(defn answer () (option i32) (drain (array.of {items})))
(defn drain (items (array i32)) (option i32)
  (let - (try (items 0u64))
    (drain (try (array.slice items 1u64 (array.length items))))))
"
        );
        let checked = check_source("try-loop.vib", &source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        vibra_interp::run(checked.program().expect("program")).expect("execution")
    };
    let short = run(3);
    let long = run(300);
    // Entering the loop and each element are one tail transfer each; the
    // empty array exits with `none`.
    assert_eq!(short.tail_transfer_count(), 4);
    assert_eq!(long.tail_transfer_count(), 301);
    assert_eq!(
        long.canonical_result(),
        "(record type: (record type: @std.option.option arguments: (array @i32)) value: (record kind: @enum type: @std.option.option variant: @none))\n"
    );
    assert_eq!(short.max_activation_depth(), long.max_activation_depth());
}
