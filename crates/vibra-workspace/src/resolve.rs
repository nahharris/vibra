//! The adapter from a captured snapshot to the resolver's input.
//!
//! Resolution receives the package and every unit's modules by value; it
//! reads no path. The units keep the snapshot's order, `@tests` last.

use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit};

use crate::snapshot::{SourceSnapshot, UnitKind};

/// The resolver's input for `snapshot`: its package and its units with the
/// exact captured bytes.
#[must_use]
pub fn resolve_input(snapshot: &SourceSnapshot) -> ResolveInput {
    let package = snapshot.project().project().package();
    let units = snapshot
        .units()
        .iter()
        .map(|unit| {
            let modules = unit
                .modules()
                .iter()
                .map(|module| {
                    SourceModule::new(module.path(), module.source_id(), module.bytes())
                })
                .collect();
            SourceUnit::new(unit.name(), unit_kind(unit.kind()), modules)
        })
        .collect();
    ResolveInput::new(PackageId::new(package.name(), package.version()), units)
}

const fn unit_kind(kind: UnitKind) -> vibra_resolve::UnitKind {
    match kind {
        UnitKind::Bin => vibra_resolve::UnitKind::Bin,
        UnitKind::Lib => vibra_resolve::UnitKind::Lib,
        UnitKind::Tests => vibra_resolve::UnitKind::Tests,
    }
}
