# Milestone 3 — Interactive evaluation and tests

An agent can evaluate ordinary Vibra forms repeatedly, try a
definition change, and run selected tests against the exact same session
revision. The terminal REPL, MCP, and editor clients use one session engine.

## Delivery order

1. Open a session against a pinned checked revision and explicit module
   context. Evaluate expressions and batches of declarations through the
   existing reader, checker, and reference interpreter. Accept a selection
   or enclosing top-level form from an editor buffer with its source origin.
   Return structured values and types, effects, diagnostics, traps, and
   captured output with revision and source origin.
2. Stage new and replacement definitions in a private overlay. Recheck
   affected callers and initializers before advancing the checked session
   revision. Failed checks
   retain the last checked revision and report draft diagnostics. Provide
   explicit inspect, reset, refresh, and close operations. Keep the session
   alive across submissions, including definitions and results still valid
   in its checked revision. A session can
   publish through M2's edit plan using the expected base head.
3. Run selected project tests or an ordinary test form staged in the session.
   Use the existing test selection and outcome model and a fresh deterministic
   host world per test. Keep session output separate from transport messages.

Close result encoding, initializer and old-value lifetime, concurrent requests,
cancellation, and resource-limit semantics in the tooling, effects, runtime,
and conformance chapters. Start with pure expression evaluation. Support
explicit run and test actions. An effectful run requires the server's execution
permission and a selected checked target that already admits its effects.
Test execution requires its own permission and uses the deterministic test
host. Expression evaluation requires explicit execution permission even for
pure code.
An unsuccessful session edit leaves the checked revision intact. Host effects
already performed by an authorized run remain observable.

- **Demo.** Open a module, send a top-level form from its buffer to the same
  running session twice with a redefinition, rerun a named test against the
  overlay, then reset and observe the original result.
- **Exit.** CLI, MCP, terminal, and editor clients use the same session
  semantics.
  A fresh CLI run/test on equivalent checked inputs agrees on observable
  outcomes. Concurrent sessions cannot see each other's drafts. Invalid forms,
  denied execution, exhausted budgets, cancellation, and close have defined
  outcomes. A later request cannot observe a half-applied redefinition.

The REPL takes inspiration from Clojure's interactive development loop.
Vibra keeps explicit module context and static effect consent.
[Clojure REPL guide](https://clojure.org/guides/repl/introduction)
