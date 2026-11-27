//! Units, modules, and declarations with their anchor identities.
//!
//! `docs/spec/04-programs-and-packages.md`, "Modules and imports": every
//! `.vib` file is one module, identified by its unit and its root-relative
//! path. `docs/spec/02-type-system.md`, "Declared types": every declaration,
//! members included, has an anchor identity; its owner path is the dotted
//! member path within the module. A field has no anchor kind and an `impl`
//! block has no name, so neither is a declaration: an `impl` is identified
//! by its target and the type or interface it is nested in, and its members
//! through it ("Declared types").

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic};
use vibra_syntax::source::{
    AssocType, Callable, CallableKind, Case, Declaration, Ident, Impl, InterfaceDecl,
    Module, ModuleLet, SourceOptions, TagSection, Test, TypeBody, TypeDecl, TypeForm,
    Use,
};
use vibra_syntax::{Document, SyntaxKind};

use crate::identity::{Anchor, DeclarationId, DeclarationKind};
use crate::input::{PackageId, ResolveInput, SourceUnit, UnitKind};
use crate::prelude::{self, ANY, SELF};

/// Who may name a declaration from another module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    /// `visibility @public`.
    Public,
    /// The default: private to its module.
    Private,
}

impl Access {
    /// The atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Public => "@public",
            Self::Private => "@private",
        }
    }

    fn of(section: Option<TagSection<'_>>) -> Self {
        match section.and_then(|section| section.tag()) {
            Some(tag) if tag.text() == "@public" => Self::Public,
            _ => Self::Private,
        }
    }
}

/// A name a module introduces at its top level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    /// A top-level declaration.
    Decl(usize),
    /// An import.
    Import(usize),
}

/// One module of a local unit or of the embedded unit.
#[derive(Debug)]
pub(crate) struct ModuleData<'a> {
    pub(crate) path: String,
    pub(crate) unit: String,
    pub(crate) unit_kind: UnitKind,
    /// The package the module belongs to: the local one, or the embedded
    /// standard library's.
    pub(crate) package: &'a PackageId,
    /// The dependency units the module may name, `@` included.
    pub(crate) dependencies: &'a [String],
    /// Whether the module is a module of the embedded standard library.
    pub(crate) embedded: bool,
    pub(crate) source_id: String,
    pub(crate) identity: DeclarationId,
    /// The flat module scope: every top-level declaration and import name.
    pub(crate) scope: BTreeMap<String, Binding>,
    /// The top-level declarations, in written order.
    pub(crate) top: Vec<usize>,
    /// The imports, in written order.
    pub(crate) imports: Vec<usize>,
    /// The `impl` blocks, in written order.
    pub(crate) impls: Vec<ImplData<'a>>,
}

/// The view a declaration was collected from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DeclView<'a> {
    Type(TypeDecl<'a>),
    Interface(InterfaceDecl<'a>),
    Callable(Callable<'a>),
    Let(ModuleLet<'a>),
    Test(Test<'a>),
    Case(Case<'a>),
    Assoc(AssocType<'a>),
}

/// One entry of an owner's member namespace.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub(crate) name: String,
    pub(crate) span: ByteSpan,
    pub(crate) kind: MemberKind,
}

/// What a member is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberKind {
    Field,
    Case,
    Callable,
    AssociatedType,
}

impl MemberKind {
    /// How a diagnostic names the member kind.
    pub(crate) const fn noun(self) -> &'static str {
        match self {
            Self::Field => "a field",
            Self::Case => "a case",
            Self::Callable => "a member function",
            Self::AssociatedType => "an associated type",
        }
    }
}

/// One declaration with its anchor.
#[derive(Debug)]
pub(crate) struct Decl<'a> {
    pub(crate) module: usize,
    pub(crate) view: DeclView<'a>,
    pub(crate) name: String,
    pub(crate) name_span: ByteSpan,
    pub(crate) span: ByteSpan,
    pub(crate) owner: Option<usize>,
    pub(crate) access: Access,
    pub(crate) anchor: Anchor,
    pub(crate) identity: DeclarationId,
    /// The member namespace of a type or interface, in written order.
    pub(crate) members: Vec<Member>,
}

impl Decl<'_> {
    pub(crate) const fn kind(&self) -> DeclarationKind {
        self.anchor.kind()
    }
}

/// One `impl` block: its owner and its view.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ImplData<'a> {
    pub(crate) owner: usize,
    pub(crate) view: Impl<'a>,
}

/// One `use`.
#[derive(Debug)]
pub(crate) struct Import<'a> {
    pub(crate) module: usize,
    pub(crate) view: Use<'a>,
    pub(crate) span: ByteSpan,
    /// The local name and the bytes that spell it: the alias, or the
    /// locator's last part.
    pub(crate) name: Option<(String, ByteSpan)>,
    pub(crate) target: ImportTarget,
}

/// What an import resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportTarget {
    /// Resolution failed and was reported.
    Unresolved,
    /// A module, local or embedded.
    Module(usize),
    /// A top-level declaration.
    Decl(usize),
}

/// The modules, declarations, and imports of one input.
#[derive(Debug)]
pub(crate) struct Graph<'a> {
    /// The local package.
    pub(crate) package: &'a PackageId,
    pub(crate) modules: Vec<ModuleData<'a>>,
    pub(crate) decls: Vec<Decl<'a>>,
    pub(crate) imports: Vec<Import<'a>>,
    /// The name of the embedded unit, `@std`.
    pub(crate) embedded_unit: &'a str,
    /// Each name of the prelude table and the declaration it resolves to.
    pub(crate) prelude: BTreeMap<String, Option<usize>>,
}

/// Parses every module of `input`, the embedded unit's in standard-library
/// mode. The reader's diagnostics go to `out`.
pub(crate) fn parse(input: &ResolveInput, out: &mut Vec<Diagnostic>) -> Vec<Document> {
    let local = input
        .units()
        .iter()
        .flat_map(SourceUnit::modules)
        .map(|module| (module, false));
    let embedded = input
        .embedded()
        .modules()
        .iter()
        .map(|module| (module, true));
    local
        .chain(embedded)
        .map(|(module, standard_library)| {
            let document = vibra_syntax::parse_source_with(
                Some(module.source_id()),
                module.bytes(),
                SourceOptions { standard_library },
            );
            out.extend(
                document.diagnostics().iter().map(|diagnostic| {
                    with_source(diagnostic.clone(), module.source_id())
                }),
            );
            document
        })
        .collect()
}

/// `diagnostic` with its source identity, unless it already names one.
pub(crate) fn with_source(diagnostic: Diagnostic, source_id: &str) -> Diagnostic {
    if diagnostic.source_id().is_some() {
        diagnostic
    } else {
        diagnostic.with_source_id(source_id)
    }
}

/// The span of the bytes `start..end` of the part at `index` of the atom
/// `text` written at `span`.
pub(crate) fn atom_part_span(
    text: &str,
    span: ByteSpan,
    index: usize,
) -> Option<ByteSpan> {
    let mut offset = span.start() + 1;
    for (position, part) in text.strip_prefix('@')?.split('.').enumerate() {
        if position == index {
            return Some(ByteSpan::new(offset, offset + part.len()));
        }
        offset += part.len() + 1;
    }
    None
}

impl<'a> Graph<'a> {
    /// Collects every module's declarations and imports. `documents` are the
    /// parsed modules of `input`, in its order.
    pub(crate) fn collect(input: &'a ResolveInput, documents: &'a [Document]) -> Self {
        let embedded = input.embedded();
        let mut graph = Self {
            package: input.package(),
            modules: Vec::new(),
            decls: Vec::new(),
            imports: Vec::new(),
            embedded_unit: embedded.name(),
            prelude: BTreeMap::new(),
        };
        let local = input.units().iter().flat_map(|unit| {
            let package = unit.package().unwrap_or(input.package());
            unit.modules().iter().map(move |module| {
                (
                    package,
                    unit.name(),
                    unit.kind(),
                    unit.dependencies(),
                    false,
                    module,
                )
            })
        });
        let standard = embedded.modules().iter().map(|module| {
            (
                embedded.package(),
                embedded.name(),
                UnitKind::Lib,
                &[][..],
                true,
                module,
            )
        });
        for ((package, unit, unit_kind, dependencies, is_embedded, module), document) in
            local.chain(standard).zip(documents)
        {
            let name = module.path().rsplit('.').next().unwrap_or_default();
            let identity = Anchor::new(
                package.name(),
                package.version(),
                module.path(),
                "",
                DeclarationKind::Module,
                name,
            )
            .identity();
            graph.modules.push(ModuleData {
                path: module.path().to_owned(),
                unit: unit.to_owned(),
                unit_kind,
                package,
                dependencies,
                embedded: is_embedded,
                source_id: module.source_id().to_owned(),
                identity,
                scope: BTreeMap::new(),
                top: Vec::new(),
                imports: Vec::new(),
                impls: Vec::new(),
            });
            let index = graph.modules.len() - 1;
            if let Some(view) = vibra_syntax::module(document) {
                graph.collect_module(index, view);
            }
        }
        graph.prelude = prelude::bind(&graph, embedded);
        graph
    }

    /// Whether `name` is a prelude name: a name of the prelude table, or a
    /// predeclared name.
    pub(crate) fn is_prelude_name(&self, name: &str) -> bool {
        name == SELF || name == ANY || self.prelude.contains_key(name)
    }

    /// Whether `module` is a module of the embedded standard library.
    pub(crate) fn is_embedded(&self, module: usize) -> bool {
        self.modules.get(module).is_some_and(|data| data.embedded)
    }

    fn collect_module(&mut self, module: usize, view: Module<'a>) {
        for declaration in view.declarations() {
            match declaration {
                Declaration::Use(view) => self.collect_use(module, view),
                Declaration::Type(view) => self.collect_type(module, view),
                Declaration::Interface(view) => self.collect_interface(module, view),
                Declaration::Fn(view) | Declaration::Proc(view) => {
                    let access = Access::of(view.visibility());
                    if let Some(index) =
                        self.collect_callable(module, None, view, access)
                    {
                        self.top(module, index);
                    }
                }
                Declaration::Let(view) => {
                    if let Some(name) = view.name() {
                        let index = self.push(
                            module,
                            None,
                            DeclView::Let(view),
                            DeclarationKind::Value,
                            name,
                            view.node().span(),
                            Access::of(view.visibility()),
                        );
                        self.top(module, index);
                    }
                }
                Declaration::Test(view) => self.collect_test(module, view),
            }
        }
    }

    fn top(&mut self, module: usize, index: usize) {
        if let Some(data) = self.modules.get_mut(module) {
            data.top.push(index);
        }
    }

    fn collect_use(&mut self, module: usize, view: Use<'a>) {
        let name = match (view.alias(), view.locator()) {
            (Some(alias), _) => Some((alias.text().to_owned(), alias.span())),
            (None, Some(locator)) => {
                let parts = locator.parts();
                let last = parts.len().saturating_sub(1);
                parts.last().and_then(|part| {
                    atom_part_span(locator.text(), locator.span(), last)
                        .map(|span| ((*part).to_owned(), span))
                })
            }
            (None, None) => None,
        };
        self.imports.push(Import {
            module,
            view,
            span: view.construct().span(),
            name,
            target: ImportTarget::Unresolved,
        });
        let index = self.imports.len() - 1;
        if let Some(data) = self.modules.get_mut(module) {
            data.imports.push(index);
        }
    }

    fn collect_type(&mut self, module: usize, view: TypeDecl<'a>) {
        let Some(name) = view.name() else {
            return;
        };
        let access = Access::of(view.visibility());
        let owner = self.push(
            module,
            None,
            DeclView::Type(view),
            DeclarationKind::Type,
            name,
            view.node().span(),
            access,
        );
        self.top(module, owner);
        let mut members = Vec::new();
        if let Some(TypeBody::Of(body)) = view.body()
            && let Some(TypeForm::Record(fields)) = body.form()
        {
            for (field, _) in fields {
                members.push(Member {
                    name: field.text().to_owned(),
                    span: field.span(),
                    kind: MemberKind::Field,
                });
            }
        }
        for case in view.cases() {
            let Some(name) = case.name() else {
                continue;
            };
            members.push(Member {
                name: name.text().to_owned(),
                span: name.span(),
                kind: MemberKind::Case,
            });
            self.push(
                module,
                Some(owner),
                DeclView::Case(case),
                DeclarationKind::Case,
                name,
                case.node().span(),
                access,
            );
        }
        for member in view.members() {
            if let Some(name) = member.name() {
                members.push(Member {
                    name: name.text().to_owned(),
                    span: name.span(),
                    kind: MemberKind::Callable,
                });
            }
            let member_access = Access::of(member.visibility());
            self.collect_callable(module, Some(owner), member, member_access);
        }
        for block in view.impls() {
            if let Some(data) = self.modules.get_mut(module) {
                data.impls.push(ImplData { owner, view: block });
            }
        }
        members.sort_by_key(|member| member.span.start());
        if let Some(decl) = self.decls.get_mut(owner) {
            decl.members = members;
        }
    }

    fn collect_interface(&mut self, module: usize, view: InterfaceDecl<'a>) {
        let Some(name) = view.name() else {
            return;
        };
        let access = Access::of(view.visibility());
        let owner = self.push(
            module,
            None,
            DeclView::Interface(view),
            DeclarationKind::Interface,
            name,
            view.node().span(),
            access,
        );
        self.top(module, owner);
        let mut members = Vec::new();
        for assoc in view.assoc_types() {
            let Some(name) = assoc.name() else {
                continue;
            };
            members.push(Member {
                name: name.text().to_owned(),
                span: name.span(),
                kind: MemberKind::AssociatedType,
            });
            self.push(
                module,
                Some(owner),
                DeclView::Assoc(assoc),
                DeclarationKind::AssociatedType,
                name,
                assoc.node().span(),
                access,
            );
        }
        for member in view.members() {
            if let Some(name) = member.name() {
                members.push(Member {
                    name: name.text().to_owned(),
                    span: name.span(),
                    kind: MemberKind::Callable,
                });
            }
            // A member is part of the interface and has its access.
            self.collect_callable(module, Some(owner), member, access);
        }
        for block in view.impls() {
            if let Some(data) = self.modules.get_mut(module) {
                data.impls.push(ImplData { owner, view: block });
            }
        }
        members.sort_by_key(|member| member.span.start());
        if let Some(decl) = self.decls.get_mut(owner) {
            decl.members = members;
        }
    }

    /// Collects a named callable with `access`: its own `visibility` at the
    /// top level and in a type, its interface's in an interface.
    fn collect_callable(
        &mut self,
        module: usize,
        owner: Option<usize>,
        view: Callable<'a>,
        access: Access,
    ) -> Option<usize> {
        let name = view.name()?;
        let kind = match view.kind() {
            CallableKind::Fn => DeclarationKind::Fn,
            CallableKind::Proc => DeclarationKind::Proc,
        };
        Some(self.push(
            module,
            owner,
            DeclView::Callable(view),
            kind,
            name,
            view.node().span(),
            access,
        ))
    }

    fn collect_test(&mut self, module: usize, view: Test<'a>) {
        let (Some(name), Some(node)) = (
            view.name_value(),
            view.construct()
                .contents()
                .iter()
                .find(|child| child.kind() == SyntaxKind::String),
        ) else {
            return;
        };
        let index = self.push_named(
            module,
            None,
            DeclView::Test(view),
            DeclarationKind::Test,
            (name, node.span()),
            view.node().span(),
            Access::Private,
        );
        self.top(module, index);
    }

    #[expect(clippy::too_many_arguments, reason = "one record's fields")]
    fn push(
        &mut self,
        module: usize,
        owner: Option<usize>,
        view: DeclView<'a>,
        kind: DeclarationKind,
        name: Ident<'_>,
        span: ByteSpan,
        access: Access,
    ) -> usize {
        self.push_named(
            module,
            owner,
            view,
            kind,
            (name.text().to_owned(), name.span()),
            span,
            access,
        )
    }

    #[expect(clippy::too_many_arguments, reason = "one record's fields")]
    fn push_named(
        &mut self,
        module: usize,
        owner: Option<usize>,
        view: DeclView<'a>,
        kind: DeclarationKind,
        (name, name_span): (String, ByteSpan),
        span: ByteSpan,
        access: Access,
    ) -> usize {
        let (package, path) = self.modules.get(module).map_or_else(
            || (self.package, String::new()),
            |data| (data.package, data.path.clone()),
        );
        let owner_path = owner
            .and_then(|owner| self.decls.get(owner))
            .map(|owner| owner.name.clone())
            .unwrap_or_default();
        let anchor = Anchor::new(
            package.name(),
            package.version(),
            path,
            owner_path,
            kind,
            name.clone(),
        );
        let identity = anchor.identity();
        self.decls.push(Decl {
            module,
            view,
            name,
            name_span,
            span,
            owner,
            access,
            anchor,
            identity,
            members: Vec::new(),
        });
        self.decls.len() - 1
    }

    /// The source identity of `module`.
    pub(crate) fn source_id(&self, module: usize) -> &str {
        self.modules
            .get(module)
            .map_or("", |data| data.source_id.as_str())
    }
}
