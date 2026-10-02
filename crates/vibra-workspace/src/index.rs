//! The `@index.v1` projection of one workspace snapshot
//! (`docs/spec/05-tooling.md`, "Index records").
//!
//! The document holds the resolved declaration, implementation, and reference
//! records of the checked snapshot, so an external retrieval consumer reads
//! every declaration's identity, contract, relations, and normalized source
//! without a second parser. It is canonical VIBON with fixed sort orders, and
//! an identical snapshot produces a byte-identical document on every host.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use vibra_diagnostics::ByteSpan;
use vibra_resolve::{DeclarationId, EntityKind, ResolvedSnapshot, Visibility};
use vibra_syntax::{Declaration, Expression, ExpressionKind, SourceAst, TypeMember};

use crate::{WorkspaceError, WorkspaceSnapshot};

/// One source span of the snapshot.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexSource {
    source_id: String,
    start: usize,
    end: usize,
}

impl IndexSource {
    fn new(source_id: &str, span: ByteSpan) -> Self {
        Self {
            source_id: source_id.to_owned(),
            start: span.start(),
            end: span.end(),
        }
    }

    /// The project-relative source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The first byte of the span.
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// The byte after the span.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }
}

/// The checked facts of one declaration, absent for an unavailable or
/// recovered one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexFacts {
    signature: String,
    effects: Vec<String>,
    errors: Vec<String>,
    applications: Vec<String>,
}

impl IndexFacts {
    /// The canonical VIBON of the signature: a checked type encoding, a
    /// generic parameter list, or `void` for a module.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// The performed effect roots, as canonical atoms without `@`, sorted.
    #[must_use]
    pub fn effects(&self) -> &[String] {
        &self.effects
    }

    /// The canonical VIBON of each error type its result can carry.
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// The identities of the declarations its body applies, without `@`,
    /// sorted and without duplicates.
    #[must_use]
    pub fn applications(&self) -> &[String] {
        &self.applications
    }
}

/// One module-level or nested declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexDeclaration {
    id: String,
    kind: &'static str,
    module: String,
    owner: Option<String>,
    visibility: &'static str,
    source: IndexSource,
    facts: Option<IndexFacts>,
    text: String,
}

impl IndexDeclaration {
    /// The canonical atom identity, without `@`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The declaration kind, without `@`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// The owning module's identity, without `@`.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The owning type, interface, or effect root of a nested member.
    #[must_use]
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// `public` or `private`.
    #[must_use]
    pub const fn visibility(&self) -> &'static str {
        self.visibility
    }

    /// The declaration's span.
    #[must_use]
    pub const fn source(&self) -> &IndexSource {
        &self.source
    }

    /// The checked facts, absent when the declaration did not check.
    #[must_use]
    pub const fn facts(&self) -> Option<&IndexFacts> {
        self.facts.as_ref()
    }

    /// The formatter-normalized source of the declaration.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One member written in an `impl` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexImplementationMember {
    contract: String,
    source: IndexSource,
    signature: String,
    text: String,
}

impl IndexImplementationMember {
    /// The identity of the contract member it implements, without `@`.
    #[must_use]
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The member's span.
    #[must_use]
    pub const fn source(&self) -> &IndexSource {
        &self.source
    }

    /// The canonical VIBON of its checked signature.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// The formatter-normalized source of the member.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One `impl` block, identified by its receiver and applied interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexImplementation {
    receiver: String,
    interface: String,
    source: IndexSource,
    text: String,
    members: Vec<IndexImplementationMember>,
}

impl IndexImplementation {
    /// The canonical VIBON of the receiver type.
    #[must_use]
    pub fn receiver(&self) -> &str {
        &self.receiver
    }

    /// The canonical VIBON of the applied interface target.
    #[must_use]
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// The block's span.
    #[must_use]
    pub const fn source(&self) -> &IndexSource {
        &self.source
    }

    /// The formatter-normalized source of the block.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The members the block writes.
    #[must_use]
    pub fn members(&self) -> &[IndexImplementationMember] {
        &self.members
    }
}

/// One resolved written name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexReference {
    from: String,
    written: String,
    source: IndexSource,
    to: Option<String>,
}

impl IndexReference {
    /// The identity of the enclosing declaration, without `@`.
    #[must_use]
    pub fn from(&self) -> &str {
        &self.from
    }

    /// The exact spelling.
    #[must_use]
    pub fn written(&self) -> &str {
        &self.written
    }

    /// The name's span.
    #[must_use]
    pub const fn source(&self) -> &IndexSource {
        &self.source
    }

    /// The resolved identity, without `@`, or `None` for a name the resolver
    /// left to the checker.
    #[must_use]
    pub fn to(&self) -> Option<&str> {
        self.to.as_deref()
    }
}

/// The `@index.v1` document of one snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexDocument {
    revision: String,
    declarations: Vec<IndexDeclaration>,
    implementations: Vec<IndexImplementation>,
    references: Vec<IndexReference>,
}

impl IndexDocument {
    /// The workspace revision the records belong to.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Declaration records, sorted by the UTF-8 bytes of their identity.
    #[must_use]
    pub fn declarations(&self) -> &[IndexDeclaration] {
        &self.declarations
    }

    /// Implementation records, sorted by receiver and then interface.
    #[must_use]
    pub fn implementations(&self) -> &[IndexImplementation] {
        &self.implementations
    }

    /// Reference records, sorted by source identity, start, and end.
    #[must_use]
    pub fn references(&self) -> &[IndexReference] {
        &self.references
    }

    /// The canonical VIBON of the document.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::from("(record\n  format: @index.v1\n");
        let _ = writeln!(output, "  revision: {}", quoted(&self.revision));
        output.push_str("  declarations: (array");
        for declaration in &self.declarations {
            let _ = write!(
                output,
                "\n    (record\n      id: @{}\n      kind: @{}\n      module: @{}\n      owner: {}\n      visibility: @{}\n      source: {}",
                declaration.id,
                declaration.kind,
                declaration.module,
                declaration
                    .owner
                    .as_ref()
                    .map_or_else(|| "void".to_owned(), |owner| format!("@{owner}")),
                declaration.visibility,
                source(&declaration.source),
            );
            if let Some(facts) = &declaration.facts {
                let _ = write!(
                    output,
                    "\n      signature: {}\n      effects: {}\n      errors: {}\n      applications: {}",
                    facts.signature,
                    atoms(&facts.effects),
                    array(&facts.errors),
                    atoms(&facts.applications),
                );
            }
            let _ = write!(output, "\n      text: {})", quoted(&declaration.text));
        }
        output.push_str(")\n  implementations: (array");
        for implementation in &self.implementations {
            let _ = write!(
                output,
                "\n    (record\n      receiver: {}\n      interface: {}\n      source: {}\n      text: {}\n      members: (array",
                implementation.receiver,
                implementation.interface,
                source(&implementation.source),
                quoted(&implementation.text),
            );
            for member in &implementation.members {
                let _ = write!(
                    output,
                    "\n        (record\n          contract: @{}\n          source: {}\n          signature: {}\n          text: {})",
                    member.contract,
                    source(&member.source),
                    member.signature,
                    quoted(&member.text),
                );
            }
            output.push_str("))");
        }
        output.push_str(")\n  references: (array");
        for reference in &self.references {
            let _ = write!(
                output,
                "\n    (record from: @{} written: {} source: {} to: {})",
                reference.from,
                quoted(&reference.written),
                source(&reference.source),
                reference
                    .to
                    .as_ref()
                    .map_or_else(|| "void".to_owned(), |to| format!("@{to}")),
            );
        }
        output.push_str("))\n");
        output
    }
}

fn quoted(text: &str) -> String {
    vibra_ir::Value::Str(text.to_owned()).canonical_vibon()
}

fn source(source: &IndexSource) -> String {
    format!(
        "(record source-id: {} start: {}u64 end: {}u64)",
        quoted(&source.source_id),
        source.start,
        source.end
    )
}

fn atoms(names: &[String]) -> String {
    array(
        &names
            .iter()
            .map(|name| format!("@{name}"))
            .collect::<Vec<_>>(),
    )
}

fn array(items: &[String]) -> String {
    if items.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", items.join(" "))
    }
}

/// The atom identity of a declaration, without `@`: unit, module, and path.
fn atom(id: &DeclarationId) -> String {
    std::iter::once(id.unit())
        .chain(id.module().iter().map(String::as_str))
        .chain(id.path().iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(".")
}

/// Where a declaration sits in its module: the top-level declaration, its
/// member, and the member of that `impl` block.
type Position = (usize, Option<usize>, Option<usize>);

/// One module's declarations by span, with the formatted module to slice
/// their normalized text from.
struct ModuleText<'a> {
    positions: BTreeMap<(usize, usize), Position>,
    original: &'a str,
    ast: &'a SourceAst,
    formatted: Option<(String, SourceAst)>,
}

impl<'a> ModuleText<'a> {
    fn new(source_id: &str, original: &'a str, ast: &'a SourceAst) -> Self {
        let mut positions = BTreeMap::new();
        for (index, declaration) in ast.declarations().iter().enumerate() {
            let span = declaration.span();
            positions.insert((span.start(), span.end()), (index, None, None));
            if let Declaration::Deffect(effect) = declaration {
                for (member_index, operation) in effect.members().iter().enumerate() {
                    let span = operation.span();
                    positions.insert(
                        (span.start(), span.end()),
                        (index, Some(member_index), None),
                    );
                }
            }
            for (member_index, member) in members(declaration).iter().enumerate() {
                let span = member_span(member);
                positions.insert(
                    (span.start(), span.end()),
                    (index, Some(member_index), None),
                );
                if let TypeMember::Implementation(block) = member {
                    for (inner_index, inner) in block.members().iter().enumerate() {
                        positions.insert(
                            (inner.span().start(), inner.span().end()),
                            (index, Some(member_index), Some(inner_index)),
                        );
                    }
                }
            }
        }
        // The formatter keeps declaration order, and the order of methods
        // and of `impl` blocks among themselves, but writes every method
        // before the first block.
        let formatted = vibra_fmt::format_source(Path::new(source_id), original)
            .ok()
            .and_then(|text| {
                let document =
                    vibra_syntax::parse_source(Path::new(source_id), &text).ok()?;
                let formatted = document.ast()?.clone();
                (formatted.declarations().len() == ast.declarations().len())
                    .then_some((text, formatted))
            });
        Self {
            positions,
            original,
            ast,
            formatted,
        }
    }

    /// The formatter-normalized text of the declaration at `span`, or its
    /// written text when the module does not format.
    fn text(&self, span: ByteSpan) -> String {
        let written = || {
            self.original
                .get(span.start()..span.end())
                .unwrap_or_default()
                .to_owned()
        };
        let Some((text, formatted)) = &self.formatted else {
            return written();
        };
        let Some(position) = self.positions.get(&(span.start(), span.end())) else {
            return written();
        };
        formatted_span(self.ast, formatted, *position)
            .and_then(|span| text.get(span.start()..span.end()))
            .map_or_else(written, str::to_owned)
    }
}

fn members(declaration: &Declaration) -> &[TypeMember] {
    match declaration {
        Declaration::Deftype(value) => value.members(),
        Declaration::Defint(value) => value.members(),
        _ => &[],
    }
}

fn member_span(member: &TypeMember) -> ByteSpan {
    match member {
        TypeMember::Method(method) => method.span(),
        TypeMember::Implementation(block) => block.span(),
    }
}

/// The member of `formatted` that the member at `index` of `written` became:
/// the method or `impl` block with the same ordinal among its own kind.
fn formatted_member<'a>(
    written: &Declaration,
    formatted: &'a Declaration,
    index: usize,
) -> Option<&'a TypeMember> {
    let written = members(written);
    let is_block =
        |member: &TypeMember| matches!(member, TypeMember::Implementation(_));
    let block = is_block(written.get(index)?);
    let ordinal = written
        .iter()
        .take(index)
        .filter(|member| is_block(member) == block)
        .count();
    members(formatted)
        .iter()
        .filter(|member| is_block(member) == block)
        .nth(ordinal)
}

fn formatted_span(
    original: &SourceAst,
    ast: &SourceAst,
    position: Position,
) -> Option<ByteSpan> {
    let (index, member, inner) = position;
    let declaration = ast.declarations().get(index)?;
    let Some(member) = member else {
        return Some(declaration.span());
    };
    // An effect root's members are operations, not type members.
    if let Declaration::Deffect(effect) = declaration {
        return effect
            .members()
            .get(member)
            .map(|operation| operation.span());
    }
    let member =
        formatted_member(original.declarations().get(index)?, declaration, member)?;
    match (member, inner) {
        (_, None) => Some(member_span(member)),
        (TypeMember::Implementation(block), Some(inner)) => {
            block.members().get(inner).map(|function| function.span())
        }
        (TypeMember::Method(_), Some(_)) => None,
    }
}

/// The body expressions of the declaration at `position`.
fn body(ast: &SourceAst, position: Position) -> Vec<&Expression> {
    let (index, member, inner) = position;
    let Some(declaration) = ast.declarations().get(index) else {
        return Vec::new();
    };
    match (declaration, member, inner) {
        (Declaration::Def(value), None, _) => vec![value.expression()],
        (Declaration::Defn(function), None, _) => {
            function.expressions().iter().collect()
        }
        (_, Some(member), None) => match members(declaration).get(member) {
            Some(TypeMember::Method(method)) => method.expressions().iter().collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The generic parameter list of a type or interface, as a VIBON array of
/// atoms.
fn generics(declaration: &Declaration) -> String {
    let attributes = match declaration {
        Declaration::Deftype(value) => value.attributes().items(),
        Declaration::Defint(value) => value.attributes().items(),
        _ => &[],
    };
    let names = attributes
        .iter()
        .filter_map(|attribute| match attribute {
            vibra_syntax::Attribute::Where(bindings) => Some(bindings),
            _ => None,
        })
        .flatten()
        .map(|binding| binding.name().value().to_owned())
        .collect::<Vec<_>>();
    atoms(&names)
}

/// Builds the `@index.v1` document of `snapshot`: every module of the local
/// package, checked with the standard library `verification` supplies.
pub fn index(
    snapshot: &WorkspaceSnapshot,
    verification: Option<&vibra_types::Stdlib>,
) -> Result<IndexDocument, WorkspaceError> {
    let graph = snapshot.source_graph()?;
    let resolved = snapshot.resolve_graph(&graph, verification)?;
    let units = graph
        .units()
        .iter()
        .map(|unit| unit.name().to_owned())
        .collect::<BTreeSet<_>>();
    let (_, selected) =
        crate::semantic::import_closure(&resolved, &units, verification);
    let selected = selected.into_iter().collect::<Vec<_>>();
    let checked = vibra_types::check_resolved(&resolved, &selected, verification);
    Ok(build(snapshot.revision().as_str(), &resolved, &checked))
}

/// Builds the `@index.v1` document of `snapshot` against the standard
/// library embedded in this toolchain.
pub fn index_with_embedded_stdlib(
    snapshot: &WorkspaceSnapshot,
) -> Result<IndexDocument, WorkspaceError> {
    let stdlib = vibra_types::load_stdlib()
        .map_err(|error| WorkspaceError::message(error.to_string()))?;
    index(snapshot, Some(&stdlib))
}

fn build(
    revision: &str,
    resolved: &ResolvedSnapshot,
    checked: &vibra_types::ResolvedCheckResult,
) -> IndexDocument {
    // Records describe the local package; the standard library is its own.
    let local = resolved
        .modules()
        .iter()
        .filter(|module| module.package() == resolved.package())
        .collect::<Vec<_>>();
    let texts = local
        .iter()
        .filter_map(|module| {
            let original = std::str::from_utf8(module.bytes()).ok()?;
            let ast = module.ast()?;
            Some((
                module.source_id(),
                (ModuleText::new(module.source_id(), original, ast), ast),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let targets = resolved
        .references()
        .iter()
        .filter_map(|reference| {
            Some((
                (
                    reference.source_id(),
                    reference.span().start(),
                    reference.span().end(),
                ),
                reference.target()?,
            ))
        })
        .collect::<BTreeMap<_, _>>();

    let mut declarations = Vec::new();
    for module in &local {
        let module_id = std::iter::once(module.unit())
            .chain(module.segments().iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(".");
        let text = texts.get(module.source_id());
        declarations.push(IndexDeclaration {
            id: module_id.clone(),
            kind: "module",
            module: module_id.clone(),
            owner: None,
            visibility: "public",
            source: IndexSource {
                source_id: module.source_id().to_owned(),
                start: 0,
                end: module.bytes().len(),
            },
            facts: Some(IndexFacts {
                signature: "void".to_owned(),
                effects: Vec::new(),
                errors: Vec::new(),
                applications: Vec::new(),
            }),
            text: text
                .and_then(|(text, _)| text.formatted.as_ref())
                .map_or_else(
                    || String::from_utf8_lossy(module.bytes()).into_owned(),
                    |(formatted, _)| formatted.clone(),
                ),
        });
        for declaration in resolved
            .declarations()
            .iter()
            .filter(|declaration| declaration.source_id() == module.source_id())
        {
            let id = declaration.id();
            let nested = id.path().len() > 1;
            let kind = match id.kind() {
                EntityKind::Value => "value",
                EntityKind::Function if nested => "method",
                EntityKind::Function => "function",
                EntityKind::Type => "type",
                EntityKind::Interface => "interface",
                EntityKind::Effect => "effect-root",
                EntityKind::Operation => "operation",
                EntityKind::Module
                | EntityKind::Test
                | EntityKind::Field
                | EntityKind::Variant => continue,
            };
            let Some((text, ast)) = text else {
                continue;
            };
            let span = declaration.span();
            let Some(position) =
                text.positions.get(&(span.start(), span.end())).copied()
            else {
                continue;
            };
            // A member written in an `impl` block is reported with its block.
            if position.2.is_some() {
                continue;
            }
            let applications = || {
                let mut applied = BTreeSet::new();
                for expression in body(ast, position) {
                    vibra_types::walk_expressions(expression, &mut |expression| {
                        if let ExpressionKind::Application(application) =
                            expression.kind()
                        {
                            let callee = application.callee().span();
                            if let Some(target) = targets.get(&(
                                module.source_id(),
                                callee.start(),
                                callee.end(),
                            )) {
                                applied.insert(atom(target));
                            }
                        }
                    });
                }
                applied.into_iter().collect::<Vec<_>>()
            };
            let facts = match kind {
                "type" | "interface" => {
                    ast.declarations()
                        .get(position.0)
                        .map(|written| IndexFacts {
                            signature: generics(written),
                            effects: Vec::new(),
                            errors: Vec::new(),
                            applications: Vec::new(),
                        })
                }
                "value" | "function" | "method" => {
                    checked.signature(id).map(|signature| IndexFacts {
                        signature: signature.signature().to_owned(),
                        effects: Vec::new(),
                        errors: signature.errors().to_vec(),
                        applications: applications(),
                    })
                }
                // Effect roots and operations are unavailable until M4.
                _ => None,
            };
            let owner = nested.then(|| {
                std::iter::once(id.unit())
                    .chain(id.module().iter().map(String::as_str))
                    .chain(
                        id.path()
                            .iter()
                            .take(id.path().len() - 1)
                            .map(String::as_str),
                    )
                    .collect::<Vec<_>>()
                    .join(".")
            });
            declarations.push(IndexDeclaration {
                id: atom(id),
                kind,
                module: module_id.clone(),
                owner,
                visibility: match declaration.visibility() {
                    Visibility::Public => "public",
                    Visibility::Private => "private",
                },
                source: IndexSource::new(module.source_id(), span),
                facts,
                text: text.text(span),
            });
        }
    }
    declarations.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));

    let interfaces = resolved
        .declarations()
        .iter()
        .map(|declaration| (declaration.id().canonical(), atom(declaration.id())))
        .collect::<BTreeMap<_, _>>();
    let mut implementations = checked
        .implementations()
        .iter()
        .filter_map(|implementation| {
            let (text, _) = texts.get(implementation.source_id())?;
            let interface = interfaces.get(implementation.interface_id())?;
            Some(IndexImplementation {
                receiver: implementation.receiver().to_owned(),
                interface: implementation.interface().to_owned(),
                source: IndexSource::new(
                    implementation.source_id(),
                    implementation.span(),
                ),
                text: text.text(implementation.span()),
                members: implementation
                    .members()
                    .iter()
                    .map(|member| IndexImplementationMember {
                        contract: format!("{interface}.{}", member.contract()),
                        source: IndexSource::new(
                            implementation.source_id(),
                            member.span(),
                        ),
                        signature: member.signature().to_owned(),
                        text: text.text(member.span()),
                    })
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    implementations.sort_by(|left, right| {
        (&left.receiver, &left.interface).cmp(&(&right.receiver, &right.interface))
    });

    // A member written in an `impl` block has no atom identity, so a name in
    // its body is enclosed by the declaration that owns the block.
    let block_members = resolved
        .declarations()
        .iter()
        .filter(|declaration| {
            texts
                .get(declaration.source_id())
                .and_then(|(text, _)| {
                    let span = declaration.span();
                    text.positions.get(&(span.start(), span.end()))
                })
                .is_some_and(|position| position.2.is_some())
        })
        .map(|declaration| declaration.id())
        .collect::<BTreeSet<_>>();
    let enclosing = |id: &DeclarationId| {
        if block_members.contains(id) {
            std::iter::once(id.unit())
                .chain(id.module().iter().map(String::as_str))
                .chain(
                    id.path()
                        .iter()
                        .take(id.path().len().saturating_sub(2))
                        .map(String::as_str),
                )
                .collect::<Vec<_>>()
                .join(".")
        } else if id.kind() == EntityKind::Test {
            // A test is not a declaration record; its references belong to
            // the test module.
            std::iter::once(id.unit())
                .chain(id.module().iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(".")
        } else {
            atom(id)
        }
    };
    let mut references = resolved
        .references()
        .iter()
        .filter(|reference| texts.contains_key(reference.source_id()))
        .map(|reference| IndexReference {
            from: enclosing(reference.from()),
            written: reference.written().to_owned(),
            source: IndexSource::new(reference.source_id(), reference.span()),
            to: reference.target().map(atom),
        })
        .collect::<Vec<_>>();
    references.sort_by(|left, right| left.source.cmp(&right.source));

    IndexDocument {
        revision: revision.to_owned(),
        declarations,
        implementations,
        references,
    }
}

#[cfg(test)]
mod tests {
    use vibra_resolve::{ResolveInput, Resolver, SourceModule, SourceUnit};

    use super::build;

    const SHAPES: &str = "(defint shape\n  visibility: @public\n  (defn sides (value self) i32))\n\n(deftype square (record side i32)\n  visibility: @public\n  (impl shape\n    (defn sides (value self) i32 4i32)))\n";
    const MAIN: &str = "(import shapes @geo.shapes)\n\n(defn count (value shapes.square) i32\n  (shapes.shape.sides value))\n";

    fn document(modules: Vec<SourceModule>) -> String {
        let resolved = Resolver::resolve(ResolveInput::new(
            "demo",
            "0.1.0",
            vec![SourceUnit::lib("geo", modules)],
        ));
        let sources = resolved
            .modules()
            .iter()
            .map(|module| module.source_id().to_owned())
            .collect::<Vec<_>>();
        let checked = vibra_types::check_resolved(&resolved, &sources, None);
        build("sha256:fixed", &resolved, &checked).canonical_vibon()
    }

    /// An identical snapshot gives a byte-identical document whatever order
    /// its sources arrive in.
    #[test]
    fn the_document_does_not_depend_on_source_order() {
        let shapes =
            || SourceModule::new("geo", ["shapes"], "src/geo/shapes.vib", SHAPES);
        let main = || SourceModule::new("geo", ["main"], "src/geo/main.vib", MAIN);
        let forward = document(vec![shapes(), main()]);
        let backward = document(vec![main(), shapes()]);
        assert_eq!(forward, backward);
        // The records are sorted: declarations by identity, with the one
        // implementation and the cross-module references present.
        let ids = forward
            .lines()
            .filter_map(|line| line.trim().strip_prefix("id: @"))
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        assert!(forward.contains("receiver: @geo.shapes.square"));
        assert!(forward.contains("contract: @geo.shapes.shape.sides"));
        assert!(forward.contains("to: @geo.shapes.shape.sides"));
    }
}
