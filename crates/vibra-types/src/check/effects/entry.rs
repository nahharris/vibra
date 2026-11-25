//! The entry of a binary target: the declaration its `entry` names, the
//! entry signature it MUST have, and the `effects` list read as a row.
//!
//! `docs/spec/04-programs-and-packages.md`, "Targets": `entry` MUST resolve
//! to a module-level `proc` with an **entry signature**: every parameter a
//! bare name and none labelled; every parameter type a **grant shape** (an
//! operation interface value type, `Option` of one, or a structural record
//! whose fields are grant shapes); the result `ExitCode` of `@std.process`;
//! and no bare `requires`. Anything else is `@project.invalidEntrySignature`
//! at the `entry` field, related to the declaration; a path that resolves
//! to another kind of entity is `@name.wrongEntityKind`, and one that
//! resolves to nothing `@name.unknownSymbol`, both at the field (the "entity
//! reference slot" of "VIBON data documents"). A generic `proc` has a type
//! parameter where a grant shape is required, so it has no entry signature.
//!
//! `effects` is "a row in data form, checked as a row is": an entry that is
//! not an operation interface, one that resolves to nothing included, is
//! `@effect.invalidReference` at that entry, and a later repeat of an
//! interface is `@effect.duplicateRoot` at the repeat, related to the first.
//!
//! A reference in data names a declaration by its path: the unit, the
//! module's parts, and then the declaration, as `docs/spec/04-programs-and-
//! packages.md`, "Modules and imports", resolves `@unit.c1…cn`. Its parts
//! after the module resolve against the module's own declarations, never
//! against its imports, so naming an entry makes no import edge and
//! changes no visibility: a private `proc` is an entry as a public one is.
//!
//! The span of "the `entry` field" and "the `effects` field" is the field's
//! value, as the decoder reports "the `format` field" (step 3 of "Schemas").
//! The admission ceiling itself, the comparison of the entry's row with the
//! list, is the workspace's (`vibra-workspace::entry`).

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{EntityRef, Library, Row, Type};
use vibra_syntax::source::{CallableKind, PatternForm};

use super::rows;
use crate::check::declare::{Entity, Program, TypeKind};

/// The module and the name of the result type of every entry.
const EXIT_CODE: (&str, &str) = ("@std.process", "ExitCode");

/// An entity reference written in a data document: the atom, `@` included,
/// and where its value is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataReference {
    path: String,
    span: ByteSpan,
}

impl DataReference {
    /// The reference `path` written at `span`.
    #[must_use]
    pub fn new(path: impl Into<String>, span: ByteSpan) -> Self {
        Self {
            path: path.into(),
            span,
        }
    }

    /// The atom, `@` included.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Where the atom is written.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// What a project file writes of one binary target for the checker to
/// resolve: its `entry`, unless the workspace already refused it, and the
/// entries of its `effects` list, all in the document `source_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetEntry {
    source_id: String,
    entry: Option<DataReference>,
    effects: Vec<DataReference>,
}

impl TargetEntry {
    /// The target whose `entry` is `entry` and whose `effects` list holds
    /// `effects`, written in the project file `source_id`. `entry` is `None`
    /// when the workspace refused it before resolution.
    #[must_use]
    pub fn new(
        source_id: impl Into<String>,
        entry: Option<DataReference>,
        effects: Vec<DataReference>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            entry,
            effects,
        }
    }
}

/// The interface, and whether the leaf is optional, of one grant leaf: an
/// operation interface value type, or `Option` of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafType {
    interface: EntityRef,
    optional: bool,
}

impl LeafType {
    /// The operation interface the leaf's grant permits.
    #[must_use]
    pub const fn interface(&self) -> &EntityRef {
        &self.interface
    }

    /// Whether the leaf's type is `Option` of the interface.
    #[must_use]
    pub const fn optional(&self) -> bool {
        self.optional
    }
}

/// The grant shape of one parameter type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantShape {
    /// An operation interface value type, or `Option` of one: one leaf.
    Leaf(LeafType),
    /// A structural record whose fields are grant shapes, in canonical
    /// field order: one leaf for each leaf of each field.
    Record(Vec<(String, Self)>),
}

/// One grant leaf of an entry: its full atom spelling, such as `@notes` or
/// `@logs.reader`, and its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryLeaf {
    name: String,
    leaf: LeafType,
}

impl EntryLeaf {
    /// The leaf's atom, `@` included: the parameter, then each field.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The leaf's interface and optionality.
    #[must_use]
    pub const fn leaf(&self) -> &LeafType {
        &self.leaf
    }
}

/// One entry of a `performs` row as written: the interface it resolves to
/// and where the entry is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowEntry {
    interface: EntityRef,
    source_id: Arc<str>,
    span: ByteSpan,
}

impl RowEntry {
    /// The operation interface.
    #[must_use]
    pub const fn interface(&self) -> &EntityRef {
        &self.interface
    }

    /// The source the entry is written in.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Where the entry is written.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The checked entry of a binary target: a module-level `proc` with an
/// entry signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntrySignature {
    function: String,
    source_id: Arc<str>,
    declaration: ByteSpan,
    row: Vec<RowEntry>,
    parameters: Vec<(String, GrantShape)>,
    parameter_spans: Vec<ByteSpan>,
}

impl EntrySignature {
    /// The qualified name of the entry's function, as
    /// [`CheckResult::functions`](crate::CheckResult::functions) records it.
    #[must_use]
    pub fn function(&self) -> &str {
        &self.function
    }

    /// The source the entry is declared in.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The entry's name in its declaration.
    #[must_use]
    pub const fn declaration(&self) -> ByteSpan {
        self.declaration
    }

    /// The entries of the entry's written `performs` row, in written order.
    #[must_use]
    pub fn row_entries(&self) -> &[RowEntry] {
        &self.row
    }

    /// The entry's written row.
    #[must_use]
    pub fn row(&self) -> Row {
        Row::new(self.row.iter().map(|entry| entry.interface.clone()))
            .unwrap_or_default()
    }

    /// Each parameter's name and grant shape, in written order.
    #[must_use]
    pub fn parameters(&self) -> &[(String, GrantShape)] {
        &self.parameters
    }

    /// Where the parameter `name` is written in the entry's declaration, in
    /// [`Self::source_id`].
    #[must_use]
    pub fn parameter_span(&self, name: &str) -> Option<ByteSpan> {
        self.parameters
            .iter()
            .zip(&self.parameter_spans)
            .find(|((parameter, _), _)| parameter == name)
            .map(|(_, span)| *span)
    }

    /// Every grant leaf, in parameter order and then in canonical field
    /// order: a parameter of an interface or of `Option` of one is one leaf
    /// named by the parameter, a record parameter one leaf for each leaf of
    /// each field, named by the parameter and the fields.
    #[must_use]
    pub fn leaves(&self) -> Vec<EntryLeaf> {
        let mut leaves = Vec::new();
        let mut pending = self
            .parameters
            .iter()
            .rev()
            .map(|(name, shape)| (format!("@{name}"), shape))
            .collect::<Vec<_>>();
        while let Some((name, shape)) = pending.pop() {
            match shape {
                GrantShape::Leaf(leaf) => leaves.push(EntryLeaf {
                    name,
                    leaf: leaf.clone(),
                }),
                GrantShape::Record(fields) => pending.extend(
                    fields
                        .iter()
                        .rev()
                        .map(|(field, shape)| (format!("{name}.{field}"), shape)),
                ),
            }
        }
        leaves
    }
}

/// What the checker found of one binary target's [`TargetEntry`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetCheck {
    diagnostics: Vec<Diagnostic>,
    signature: Option<EntrySignature>,
    ceiling: Option<Vec<(EntityRef, ByteSpan)>>,
}

impl TargetCheck {
    /// The faults of the entry and of the `effects` entries, in the project
    /// file, each with its source.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The entry, when it resolves to a `proc` with an entry signature.
    #[must_use]
    pub const fn signature(&self) -> Option<&EntrySignature> {
        self.signature.as_ref()
    }

    /// The `effects` list as a row, with where each entry is written, when
    /// every entry names an operation interface once.
    #[must_use]
    pub fn ceiling(&self) -> Option<&[(EntityRef, ByteSpan)]> {
        self.ceiling.as_deref()
    }
}

/// What a data reference resolves to.
enum Found {
    /// Nothing: no module, or no declaration or member of that name.
    Nothing,
    /// A module.
    Module,
    /// A top-level declaration of the module.
    Declaration(Entity),
    /// A callable member of a type or an interface, written at `name` in
    /// `module`.
    Member { module: usize, name: ByteSpan },
    /// A case of a variant type.
    Case,
    /// A field of a record type.
    Field,
}

/// The module of the declaration `entity`, `None` for a module.
fn module_of(program: &Program<'_>, entity: Entity) -> Option<usize> {
    match entity {
        Entity::Type(index) => program.types.get(index).map(|data| data.module),
        Entity::Interface(index) => {
            program.interfaces.get(index).map(|data| data.module)
        }
        Entity::Fn(index) => program.functions.get(index).map(|data| data.module),
        Entity::Value(index) => program.values.get(index).map(|data| data.module),
        Entity::Module(_) => None,
    }
}

/// Resolves the reference `path`: the walk stops at the first prefix of
/// its parts that is a module, and the parts after it name one of that
/// module's own declarations and, at most, one member, case, or field of
/// it.
fn resolve(program: &Program<'_>, path: &str) -> Found {
    let parts = path
        .strip_prefix('@')
        .unwrap_or(path)
        .split('.')
        .collect::<Vec<_>>();
    let module = (1..=parts.len()).find_map(|length| {
        let prefix = parts.get(..length)?;
        let candidate = format!("@{}", prefix.join("."));
        program
            .modules
            .iter()
            .position(|module| module.path == candidate)
            .map(|index| (index, parts.get(length..).unwrap_or_default()))
    });
    let Some((module, rest)) = module else {
        return Found::Nothing;
    };
    let Some((first, rest)) = rest.split_first() else {
        return Found::Module;
    };
    let Some(entity) = program
        .modules
        .get(module)
        .and_then(|data| data.scope.get(*first).copied())
        .filter(|entity| module_of(program, *entity) == Some(module))
    else {
        return Found::Nothing;
    };
    match rest {
        [] => Found::Declaration(entity),
        [part] => member(program, entity, part),
        _ => Found::Nothing,
    }
}

/// The member, case, or field `part` of the declaration `entity`.
fn member(program: &Program<'_>, entity: Entity, part: &str) -> Found {
    match entity {
        Entity::Type(index) => {
            let Some(data) = program.types.get(index) else {
                return Found::Nothing;
            };
            if let Some(name) = data
                .members
                .get(part)
                .and_then(|function| program.functions.get(*function))
                .and_then(|function| function.view.name())
            {
                return Found::Member {
                    module: data.module,
                    name: name.span(),
                };
            }
            match &data.kind {
                TypeKind::Variant(_) if data.case(part).is_some() => Found::Case,
                TypeKind::Record(fields)
                    if fields.iter().any(|(name, _)| name == part) =>
                {
                    Found::Field
                }
                _ => Found::Nothing,
            }
        }
        Entity::Interface(index) => program
            .interfaces
            .get(index)
            .and_then(|data| {
                data.view
                    .members()
                    .iter()
                    .filter_map(|member| member.name())
                    .find(|name| name.text() == part)
                    .map(|name| Found::Member {
                        module: data.module,
                        name: name.span(),
                    })
            })
            .unwrap_or(Found::Nothing),
        Entity::Fn(_) | Entity::Value(_) | Entity::Module(_) => Found::Nothing,
    }
}

/// How a diagnostic names what a reference resolved to, other than a
/// callable.
fn kind_of(found: &Found) -> &'static str {
    match found {
        Found::Module => "a module",
        Found::Declaration(Entity::Type(_)) => "a type",
        Found::Declaration(Entity::Interface(_)) => "an interface",
        Found::Declaration(Entity::Value(_)) => "a module value",
        Found::Case => "a case",
        Found::Field => "a field",
        Found::Declaration(Entity::Fn(_) | Entity::Module(_))
        | Found::Member { .. }
        | Found::Nothing => "something else",
    }
}

/// Checks every target of `targets` against `program`, in order.
pub(crate) fn check_targets(
    program: &Program<'_>,
    targets: &[TargetEntry],
) -> Vec<TargetCheck> {
    targets
        .iter()
        .map(|target| {
            let mut diagnostics = Vec::new();
            let signature =
                target.entry.as_ref().and_then(|entry| {
                    match entry_signature(program, entry) {
                        Ok(signature) => signature,
                        Err(diagnostic) => {
                            diagnostics.push(*diagnostic);
                            None
                        }
                    }
                });
            let ceiling = ceiling(
                program,
                &target.source_id,
                &target.effects,
                &mut diagnostics,
            );
            TargetCheck {
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.with_source_id(&target.source_id))
                    .collect(),
                signature,
                ceiling,
            }
        })
        .collect()
}

/// The entry `entry` resolves to, when it is a module-level `proc` with an
/// entry signature; `Ok(None)` when its header has a fault of its own,
/// reported where the header is read. Any other declaration is the one
/// diagnostic of the entry.
pub(crate) fn entry_signature(
    program: &Program<'_>,
    entry: &DataReference,
) -> Result<Option<EntrySignature>, Box<Diagnostic>> {
    let span = entry.span();
    let path = entry.path();
    let invalid = |module: usize, name: ByteSpan, why: &str| {
        Box::new(program.related(
            Diagnostic::new(
                DiagnosticCode::ProjectInvalidEntrySignature,
                span,
                format!("`{path}` has no entry signature: {why}"),
            ),
            module,
            name,
            "the declaration",
        ))
    };
    let found = resolve(program, path);
    let index = match found {
        Found::Nothing => {
            return Err(Box::new(Diagnostic::new(
                DiagnosticCode::NameUnknownSymbol,
                span,
                format!("`{path}` names nothing"),
            )));
        }
        Found::Member { module, name } => {
            return Err(invalid(
                module,
                name,
                "it is a member, and the entry is a module-level `proc`",
            ));
        }
        Found::Declaration(Entity::Fn(index)) => index,
        Found::Module | Found::Declaration(_) | Found::Case | Found::Field => {
            return Err(Box::new(Diagnostic::new(
                DiagnosticCode::NameWrongEntityKind,
                span,
                format!(
                    "`{path}` names {}; the entry names a module-level `proc`",
                    kind_of(&found)
                ),
            )));
        }
    };
    let Some(data) = program.functions.get(index) else {
        return Ok(None);
    };
    let name = data
        .view
        .name()
        .map_or(data.view.construct().span(), |name| name.span());
    if data.view.kind() != CallableKind::Proc {
        return Err(invalid(
            data.module,
            name,
            "it is a `fn`, and the entry is a `proc`",
        ));
    }
    let Some(signature) = data.signature.as_ref() else {
        return Ok(None);
    };
    if !data.binders.is_empty() {
        return Err(invalid(
            data.module,
            name,
            "a type parameter is no grant shape, so the entry has none",
        ));
    }
    if !data.view.labelled().is_empty() {
        return Err(invalid(
            data.module,
            name,
            "the entry has no labelled parameters",
        ));
    }
    if data
        .view
        .requires()
        .is_some_and(|guard| guard.consequence().is_none())
    {
        return Err(invalid(
            data.module,
            name,
            "no call site could discharge a bare `requires` on the entry",
        ));
    }
    let mut parameters = Vec::new();
    let mut parameter_spans = Vec::new();
    for ((pattern, _), ty) in data.view.takes().iter().zip(&signature.parameters) {
        let parameter = match pattern.form() {
            PatternForm::Path(path) => match path.segments().as_slice() {
                [only] => Some((only.text().to_owned(), only.span())),
                _ => None,
            },
            _ => None,
        };
        let Some((parameter, parameter_span)) = parameter else {
            return Err(invalid(data.module, name, "every parameter is a bare name"));
        };
        let Some(shape) = grant_shape(program, ty, 0) else {
            return Err(invalid(
                data.module,
                name,
                "every parameter type is a grant shape: an operation interface, \
                 `Option` of one, or a record of grant shapes",
            ));
        };
        parameters.push((parameter, shape));
        parameter_spans.push(parameter_span);
    }
    if !is_exit_code(program, &signature.result) {
        return Err(invalid(
            data.module,
            name,
            "the result type is `ExitCode` from `@std.process`",
        ));
    }
    let source_id = program
        .modules
        .get(data.module)
        .map_or_else(|| Arc::from(""), |module| Arc::clone(&module.source_id));
    let mut row = Vec::new();
    for written in data.view.performs().iter().flat_map(|row| row.paths()) {
        let Some(Entity::Interface(interface)) =
            program.path_entity(data.module, &written)
        else {
            return Ok(None);
        };
        let Some(id) = program
            .interfaces
            .get(interface)
            .and_then(|interface| interface.id.as_ref())
        else {
            return Ok(None);
        };
        row.push(RowEntry {
            interface: id.entity().clone(),
            source_id: Arc::clone(&source_id),
            span: written.node().span(),
        });
    }
    Ok(Some(EntrySignature {
        function: data.qualified.clone(),
        source_id,
        declaration: name,
        row,
        parameters,
        parameter_spans,
    }))
}

/// Whether `id` is an operation interface of `program`.
fn operation_interface(program: &Program<'_>, id: &vibra_ir::TypeId) -> bool {
    program
        .interfaces
        .iter()
        .position(|data| data.id.as_ref() == Some(id))
        .is_some_and(|index| rows::is_operation_interface(program, index))
}

/// The grant shape of the parameter type `ty`, at `depth` records inside
/// the parameter: `None` when it is none. A declared type over a grant, a
/// tuple, a list, `Option` of anything but an interface, and a type
/// parameter are none. A written type nests at most [`Type::MAX_DEPTH`]
/// levels, so the walk is bounded by that constant.
fn grant_shape(program: &Program<'_>, ty: &Type, depth: usize) -> Option<GrantShape> {
    if depth > Type::MAX_DEPTH {
        return None;
    }
    match ty {
        Type::Interface(id, ..) if operation_interface(program, id) => {
            Some(GrantShape::Leaf(LeafType {
                interface: id.entity().clone(),
                optional: false,
            }))
        }
        Type::Declared(id, arguments)
            if program.library_of(id) == Some(Library::Option) =>
        {
            match arguments.as_slice() {
                [Type::Interface(inner, ..)] if operation_interface(program, inner) => {
                    Some(GrantShape::Leaf(LeafType {
                        interface: inner.entity().clone(),
                        optional: true,
                    }))
                }
                _ => None,
            }
        }
        Type::Record(fields) => fields
            .entries()
            .iter()
            .map(|(field, ty)| {
                grant_shape(program, ty, depth + 1).map(|shape| (field.clone(), shape))
            })
            .collect::<Option<Vec<_>>>()
            .map(GrantShape::Record),
        _ => None,
    }
}

/// Whether `ty` is `ExitCode` of the embedded module `@std.process`.
fn is_exit_code(program: &Program<'_>, ty: &Type) -> bool {
    let Type::Declared(id, arguments) = ty else {
        return false;
    };
    let (module, name) = EXIT_CODE;
    arguments.is_empty()
        && program
            .modules
            .iter()
            .find(|data| data.embedded && data.path == module)
            .and_then(|data| data.scope.get(name).copied())
            .and_then(|entity| match entity {
                Entity::Type(index) => program.types.get(index),
                _ => None,
            })
            .and_then(|data| data.id.as_ref())
            == Some(id)
}

/// The `effects` list `effects` of the project file `source_id` as a row,
/// when every entry names an operation interface and none repeats; each
/// fault is pushed onto `diagnostics`.
fn ceiling(
    program: &Program<'_>,
    source_id: &str,
    effects: &[DataReference],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<(EntityRef, ByteSpan)>> {
    let mut roots: Vec<(EntityRef, ByteSpan)> = Vec::new();
    let mut complete = true;
    for entry in effects {
        let span = entry.span();
        let path = entry.path();
        let found = resolve(program, path);
        let why = match &found {
            Found::Declaration(Entity::Interface(index))
                if rows::is_operation_interface(program, *index) =>
            {
                // An interface this checker does not support has its
                // `@tool.unavailable` at its declaration, and no row of it.
                let Some(id) = program
                    .interfaces
                    .get(*index)
                    .and_then(|data| data.id.as_ref())
                else {
                    complete = false;
                    continue;
                };
                if let Some((_, first)) =
                    roots.iter().find(|(root, _)| root == id.entity())
                {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::EffectDuplicateRoot,
                            span,
                            format!("`{path}` is named twice in `effects`"),
                        )
                        .with_related_source(
                            source_id,
                            *first,
                            "the first",
                        ),
                    );
                    complete = false;
                } else {
                    roots.push((id.entity().clone(), span));
                }
                continue;
            }
            Found::Declaration(Entity::Interface(_)) => {
                format!(
                    "`{path}` declares no `proc` member, so it is no operation interface"
                )
            }
            Found::Nothing => {
                format!("`{path}` names nothing; `effects` names operation interfaces")
            }
            Found::Member { .. } | Found::Declaration(Entity::Fn(_)) => {
                format!(
                    "`{path}` names a callable; `effects` names operation interfaces"
                )
            }
            other => format!(
                "`{path}` names {}; `effects` names operation interfaces",
                kind_of(other)
            ),
        };
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::EffectInvalidReference,
            span,
            why,
        ));
        complete = false;
    }
    complete.then_some(roots)
}
