//! The predeclared prelude names and the binding of the prelude table.
//!
//! `docs/spec/02-type-system.md`, "The prelude": the compiler-owned types,
//! the types and interfaces that play a role, `From`, `TryFrom`, `Any`,
//! `Self`, `Ordering`, and the values `true` and `false` are the only
//! declarations a program names without an import. A prelude name resolves
//! to its real declaration in every module. The set is closed.
//!
//! The embedded unit's prelude table names the declaration of every prelude
//! name but two: `Any` and `Self` are predeclared and have no declaration.
//! `Self` denotes the receiver type and is visible only inside a `type`, an
//! `interface`, and an `impl` (`docs/spec/02-type-system.md`, "Dispatch");
//! `Any` is the interface every type satisfies.

use std::collections::BTreeMap;

use crate::graph::Graph;
use crate::identity::DeclarationKind;
use crate::input::EmbeddedUnit;

/// The receiver type's name.
pub(crate) const SELF: &str = "Self";

/// The predeclared interface every type satisfies.
pub(crate) const ANY: &str = "Any";

/// Each name of `embedded`'s prelude table, with the top-level declaration
/// of the embedded unit whose identity the table gives; `None` when no
/// declaration has that identity.
pub(crate) fn bind(
    graph: &Graph<'_>,
    embedded: &EmbeddedUnit,
) -> BTreeMap<String, Option<usize>> {
    embedded
        .prelude()
        .iter()
        .map(|binding| {
            let decl = graph.decls.iter().position(|decl| {
                decl.owner.is_none()
                    && decl.kind() != DeclarationKind::Test
                    && graph
                        .modules
                        .get(decl.module)
                        .is_some_and(|module| module.embedded)
                    && decl.identity.as_str() == binding.identity()
            });
            (binding.name().to_owned(), decl)
        })
        .collect()
}
