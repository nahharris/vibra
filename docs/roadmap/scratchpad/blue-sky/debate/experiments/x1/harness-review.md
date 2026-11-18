# Codex pre-build card review

Status: proposed clarifications, before harness implementation; 2026-11-15.

The four complete examples appear parseable and semantically matched. The
decisions table is implementable. These clarifications must be explicit before
participant runs; none requires changing the example algorithms.

1. `option.some(value)` is positional, unlike user record/variant constructors.
   `some`/`none` match arms narrow the option and expose `value` only in `some`.
2. K's `let` and `return` are statement sequences without closers. D0's value
   `(if c a b)` is a special form, an exception to “parentheses only for calls”.
3. Propose JSON string escapes, UTF-8 Unicode scalar text, and identifiers
   `[a-z][a-z0-9]*(?:-[a-z0-9]+)*`, qualified by dots. D0 accepts LF/CRLF,
   two-space indentation and blank/comment lines; tabs in indentation fail.
4. B constructor braces are recognized by a declared type/case head and labeled
   fields. Propose excluding empty record types from this subset: empty variant
   cases already use `case-name()`. This avoids `if flag {}` ambiguity.
5. Propose top-level forward function references and self recursion; nested
   functions become visible at their declaration and can refer to themselves,
   capture preceding bindings, and call top-level functions. No rebinding in a
   lexical frame. Please settle whether outer-binding shadowing is also banned;
   a recursive helper often reuses a parent parameter name. I recommend allowing
   parameter shadowing in a nested function while banning duplicate bindings
   in each frame, and documenting that consistently in all cards.
6. Named functions are values inferred from their declarations for transforms;
   these tasks need no user-written function type or lambda syntax.
7. The experimental checker checks types, arities, declared constructor fields,
   variant narrowing/exhaustiveness, and return paths. It does not claim a
   termination/refinement proof. Requires/ensures/where are executable guards;
   evaluation is fuel-limited. T5's nonnegative quantities are a test-domain
   assumption, not an unstated proof obligation. Examples run through the same
   evaluator and contracts.
8. README's fresh context per task supersedes tasks.md's earlier T1–T7 batch.
   Extraction accepts plain source or one enclosing code fence, without prose,
   repairs, or multiple fences. T10 is an exact supplied prefix plus suffix;
   no newline is invented. T9 is a complete replacement.
9. `requires-violated` cases pass only on the named entry precondition fault,
   not on arbitrary runtime/type/ensures/fuel failures. Declared task constraints
   (T4 banned transforms, T7 two try sites, required contract/example clauses)
   need a separate structural task check as well as result tests.
10. Proposed manifest: schema 1, trials with id/surface/task/raw_path, optional
    prefix_path and usage metadata. Paths are relative to the manifest. Hidden
    test directory is an explicit runner argument; no fixture discovery outside
    that argument. Normal results preserve statuses/hashes/case ids without
    printing hidden expected values.

Canonical trees exclude comments and source spans. Function/declaration order
and parameter order are retained; labeled constructor fields are normalized by
field name, so harmless surface field ordering does not change the tree.

Please confirm/amend items 3–5 before grammar freeze. I can build the remaining
plumbing independently while these are settled. No participant tasks have run.
