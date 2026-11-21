//! The performed row of every checked body, against its ceiling.
//!
//! `docs/spec/03-effects.md`, "Effect ceilings" and "Cold plans and
//! performed rows": a body's row is the union, over its `await` nodes, of
//! the row of the awaited plan's `Intent` type. Step 09 fills this file with
//! the rows, their witnesses, and `@effect.outsideCeiling`. Until then no
//! ceiling is checked, so an `await` of a plan whose row is not empty is
//! `@tool.unavailable` at the `await` and never accepted.

use vibra_diagnostics::{Diagnostic, DiagnosticCode};

use crate::check::declare::Program;
use crate::check::expr::Target;
use crate::check::lower::Pass;

/// Checks the rows of the bodies of `pass`, once per checked program, after
/// every body is typed: each fault joins the faults of the declaration
/// whose body holds the `await`.
pub(crate) fn check_bodies(program: &Program<'_>, pass: &mut Pass) {
    let bodies = pass
        .functions
        .iter()
        .map(|(index, checked)| (Target::Function(*index), checked))
        .chain(
            pass.values
                .iter()
                .map(|(index, checked)| (Target::Value(*index), checked)),
        )
        .chain(
            pass.tests
                .iter()
                .map(|(index, checked)| (Target::Test(*index), checked)),
        );
    let mut found = Vec::new();
    for (target, checked) in bodies {
        let module = match target {
            Target::Function(index) => {
                program.functions.get(index).map(|data| data.module)
            }
            Target::Value(index) => program.values.get(index).map(|data| data.module),
            Target::Test(index) => program.tests.get(index).map(|data| data.module),
        };
        let source = module
            .and_then(|module| program.modules.get(module))
            .map_or("", |data| data.source_id.as_ref());
        for (span, plan) in &checked.awaits {
            if plan.effects.roots().is_empty() {
                continue;
            }
            found.push((
                target,
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    *span,
                    "an `await` of a plan whose row is not empty is not supported by \
                     this checker yet",
                )
                .with_source_id(source),
            ));
        }
    }
    for (target, diagnostic) in found {
        pass.faults.entry(target).or_default().push(diagnostic);
    }
}
