//! The adapter from a captured snapshot to the resolver's input.
//!
//! Resolution receives the package, every unit's modules by value, and the
//! embedded standard library; it reads no path. The units keep the
//! snapshot's order, `@tests` last. A dependency's unit carries its own
//! package, the provenance of its declarations' identities, and each unit
//! carries the dependency units its modules may name.

use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit};
use vibra_types::{StdlibError, load_stdlib};

use crate::snapshot::{SourceFile, SourceSnapshot, UnitKind};

/// The resolver's input for `snapshot`: its package, its units with the
/// exact captured bytes, and the embedded standard library.
///
/// # Errors
///
/// The embedded standard library is defective, a toolchain defect reported
/// as `@runtime.invalidCheckedProgram`.
pub fn resolve_input(snapshot: &SourceSnapshot) -> Result<ResolveInput, StdlibError> {
    resolve_input_of(snapshot, &|_| true)
}

/// The resolver's input for the modules of `snapshot` that `keep` admits.
///
/// # Errors
///
/// The embedded standard library is defective.
pub(crate) fn resolve_input_of(
    snapshot: &SourceSnapshot,
    keep: &dyn Fn(&SourceFile) -> bool,
) -> Result<ResolveInput, StdlibError> {
    let stdlib = load_stdlib()?;
    let package = snapshot.project().project().package();
    let units = snapshot
        .units()
        .iter()
        .map(|unit| {
            let modules = unit
                .modules()
                .iter()
                .filter(|module| keep(module))
                .map(|module| {
                    SourceModule::new(module.path(), module.source_id(), module.bytes())
                })
                .collect();
            let source = SourceUnit::new(unit.name(), unit_kind(unit.kind()), modules)
                .with_dependencies(unit.dependencies().to_vec());
            match unit.package() {
                Some(package) => source
                    .with_package(PackageId::new(package.name(), package.version())),
                None => source,
            }
        })
        .collect();
    Ok(ResolveInput::new(
        PackageId::new(package.name(), package.version()),
        units,
        stdlib.overlay().embedded_unit(),
    ))
}

const fn unit_kind(kind: UnitKind) -> vibra_resolve::UnitKind {
    match kind {
        UnitKind::Bin => vibra_resolve::UnitKind::Bin,
        UnitKind::Lib => vibra_resolve::UnitKind::Lib,
        UnitKind::Tests => vibra_resolve::UnitKind::Tests,
        UnitKind::Dependency => vibra_resolve::UnitKind::Dependency,
    }
}
