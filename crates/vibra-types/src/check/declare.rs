//! Declarations: every module of the checked graph, its names, and the
//! headers of the declarations this checker supports.
//!
//! `docs/spec/02-type-system.md`, "Declared types", "The prelude", and
//! "Namespaces and resolution", and `docs/spec/01-source-language.md`,
//! "Declarations". Resolution has already reported every name fault, so the
//! names here are looked up without reporting: a name that does not resolve
//! leaves an error type that reports nothing again.
//!
//! The checker supports `fn` declarations, and `proc` declarations whose row
//! is empty, with parameter patterns, labelled parameters, bounded type
//! parameters, `requires` and `ensures`, and written
//! types of every form but `Proc` and `Intent`; module values of those types;
//! wrappers, declared tuples, declared records, and variant types, generic or
//! not and with bounded parameters, with their members, a wrapper, declared
//! tuple, or declared record with a `where` included; interfaces with their
//! parameters, associated types,
//! abstract and default members, and their implementations in a type or in
//! the interface;
//! and, in the embedded standard library, the intrinsic scalar types and
//! `List(T)` with their external members. A procedure whose row is not
//! empty is `@tool.unavailable` at its `performs` section; a member that
//! performs operations, and any other unsupported declaration of a local
//! module, over the declaration; one of the standard library is skipped
//! with a note.
//!
//! A written type nests at most [`Type::MAX_DEPTH`] levels, measured before
//! it is converted; an applied type supplies exactly its head's parameters,
//! each meeting its parameter's bound; and the key of a `Dict` implements
//! `Ordered`. The bounds and an open key are decided once every
//! implementation is known, under the bounds in scope where the type is
//! written (a [`BoundGoal`]). An associated type a bound leaves open is one
//! more type parameter of the `fn`, member, anonymous callable, or
//! implementation the bound is on, abstract inside it and fixed at each use
//! by the implementation the bound selects ([`Program::open_choices`]).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::compiler::CompilerIntrinsic;
use vibra_ir::{FnType, Library, NamedTypes, Type, TypeId};
use vibra_syntax::source::{
    Callable, Declaration, ExprForm, Given, Impl, Inputs, InterfaceDecl, Module,
    ModuleLet, Primary, SourceOptions, Suffix, Test, TypeBody, TypeDecl, TypeExpr,
    TypeForm, Where,
};
use vibra_syntax::{CstNode, Document, parse_source, parse_source_with};

use super::effects;
use crate::stdlib::declaration_identity;

/// One module handed to the checker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    /// The module path atom, `@` included.
    pub(crate) path: String,
    /// The source identity.
    pub(crate) source_id: String,
    /// The exact bytes.
    pub(crate) bytes: Vec<u8>,
    /// Whether the module belongs to the embedded standard library.
    pub(crate) embedded: bool,
    /// Whether the module belongs to the `@tests` unit.
    pub(crate) tests: bool,
    /// The package of a dependency's module; `None` for a local one.
    pub(crate) package: Option<PackageName>,
}

/// Parses every source in the grammar of where it comes from.
pub(crate) fn parse_all(sources: &[Source]) -> Vec<Document> {
    sources
        .iter()
        .map(|source| {
            if source.embedded {
                parse_source_with(
                    Some(&source.source_id),
                    &source.bytes,
                    SourceOptions {
                        standard_library: true,
                    },
                )
            } else {
                parse_source(Some(&source.source_id), &source.bytes)
            }
        })
        .collect()
}

/// What a module-scope name denotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entity {
    /// A `type`.
    Type(usize),
    /// An `interface`.
    Interface(usize),
    /// A `fn`.
    Fn(usize),
    /// A module value.
    Value(usize),
    /// A module, through an import.
    Module(usize),
}

/// One module.
#[derive(Debug)]
pub(crate) struct ModuleData<'a> {
    /// The module path atom, `@` included.
    pub(crate) path: String,
    /// The source identity.
    pub(crate) source_id: Arc<str>,
    /// Whether it belongs to the embedded standard library.
    pub(crate) embedded: bool,
    /// Whether it belongs to the `@tests` unit.
    pub(crate) tests: bool,
    /// The package of a dependency's module; `None` for a local one.
    pub(crate) package: Option<PackageName>,
    /// Whether its declarations are checked and lowered.
    pub(crate) in_scope: bool,
    /// The typed view.
    pub(crate) view: Option<Module<'a>>,
    /// The document text, when it decoded.
    pub(crate) text: Option<&'a str>,
    /// The flat module scope: the top-level declarations and the imports.
    pub(crate) scope: BTreeMap<String, Entity>,
}

/// One case of a variant type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CaseData {
    pub(crate) name: String,
    /// The named fields in declaration order, `None` for a case without
    /// fields. A type parameter is a [`Type::Binder`].
    pub(crate) fields: Option<Vec<(String, Type)>>,
}

/// The shape of a declared type. A type parameter in a representation is a
/// [`Type::Binder`] at its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeKind {
    /// An intrinsic scalar type of the standard library.
    Scalar(Type),
    /// The standard library's `List(T)`.
    List,
    /// A wrapper over its representation.
    Wrapper(Type),
    /// A declared tuple over its components.
    Tuple(Vec<Type>),
    /// A declared record over its fields, in declaration order.
    Record(Vec<(String, Type)>),
    /// A variant type over its cases, in declaration order.
    Variant(Vec<CaseData>),
    /// A type whose body is still being read.
    Pending,
    /// A type whose body has a fault, reported once at the body.
    Faulty,
    /// Any other type.
    Unsupported,
}

/// One `type`.
#[derive(Debug)]
pub(crate) struct TypeData<'a> {
    pub(crate) module: usize,
    pub(crate) name: String,
    pub(crate) view: TypeDecl<'a>,
    pub(crate) identity: String,
    pub(crate) kind: TypeKind,
    /// The checked IR identity of a declared type.
    pub(crate) id: Option<TypeId>,
    /// The type parameters, in written order.
    pub(crate) params: Vec<String>,
    /// The bound of each parameter, `None` for `Any`.
    pub(crate) bounds: Vec<Option<BoundData>>,
    /// Whether the representation is visible in every module that sees the
    /// type: the `representation` section, or else the type's own
    /// visibility.
    pub(crate) representation_public: bool,
    /// Whether a `where` states an invariant of the type.
    pub(crate) invariant: bool,
    /// Whether that `where` has an `else`, so that the constructor returns
    /// a `Result`.
    pub(crate) fallible: bool,
    /// The members, by name.
    pub(crate) members: BTreeMap<String, usize>,
}

impl TypeData<'_> {
    /// The type the declaration denotes inside itself: applied to its own
    /// parameters, when this checker supports it.
    pub(crate) fn self_type(&self) -> Option<Type> {
        match (&self.kind, &self.id) {
            (TypeKind::Scalar(ty), _) => Some(ty.clone()),
            (TypeKind::List, _) => Some(Type::list(Type::Binder(0))),
            (TypeKind::Unsupported | TypeKind::Faulty, _) | (_, None) => None,
            (_, Some(id)) => Some(Type::Declared(
                id.clone(),
                (0..self.params.len()).map(Type::Binder).collect(),
            )),
        }
    }

    /// The type the declaration denotes applied to `arguments`.
    pub(crate) fn applied(&self, arguments: Vec<Type>) -> Option<Type> {
        match (&self.kind, &self.id) {
            (TypeKind::Scalar(ty), _) => Some(ty.clone()),
            (TypeKind::List, _) => arguments.into_iter().next().map(Type::list),
            (TypeKind::Unsupported | TypeKind::Faulty, _) | (_, None) => None,
            (_, Some(id)) => Some(Type::Declared(id.clone(), arguments)),
        }
    }

    /// The cases of a variant type.
    pub(crate) fn cases(&self) -> Option<&[CaseData]> {
        match &self.kind {
            TypeKind::Variant(cases) => Some(cases),
            _ => None,
        }
    }

    /// The case `name` of a variant type.
    pub(crate) fn case(&self, name: &str) -> Option<&CaseData> {
        self.cases()?.iter().find(|case| case.name == name)
    }
}

/// The type parameters a written type may name, what `Self` denotes, and
/// the associated type names in scope with what each denotes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scope<'s> {
    pub(crate) self_type: Option<&'s Type>,
    /// Each name in scope stands for the [`Type::Binder`] at its position.
    pub(crate) binders: &'s [String],
    /// The bare associated names: inside an interface each is its own
    /// [`Type::Associated`], inside an `impl` each is the chosen type.
    pub(crate) associated: &'s [(String, Type)],
}

/// The bound of a type parameter: an interface at its positional
/// arguments, with the choices its labels fix, in canonical name order. A
/// type parameter of the header is a [`Type::Binder`] into its binders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundData {
    pub(crate) interface: usize,
    pub(crate) arguments: Vec<Type>,
    pub(crate) choices: Vec<(String, Type)>,
    /// The written bound.
    pub(crate) span: ByteSpan,
}

/// One `fn`: top level, a member of a type, a member of an `impl`, or the
/// default member of an interface.
#[derive(Debug)]
pub(crate) struct FnData<'a> {
    pub(crate) module: usize,
    /// The type it is a member of.
    pub(crate) owner: Option<usize>,
    /// The interface it implements, for a member of an `impl`.
    pub(crate) implements: Option<usize>,
    /// The interface whose default member it is.
    pub(crate) contract: Option<usize>,
    pub(crate) name: String,
    pub(crate) view: Callable<'a>,
    pub(crate) identity: String,
    /// The qualified name the checked IR records.
    pub(crate) qualified: String,
    /// The type parameters in scope: the owner's, then its own.
    pub(crate) binders: Vec<String>,
    /// The bound of each binder, `None` for `Any`.
    pub(crate) bounds: Vec<Option<BoundData>>,
    /// What `Self` denotes in its header and body.
    pub(crate) self_type: Option<Type>,
    /// The associated names in scope in its header and body.
    pub(crate) associated: Vec<(String, Type)>,
    /// The signature, when this checker supports the header; a type
    /// parameter is a [`Type::Binder`] into `binders`.
    pub(crate) signature: Option<FnType>,
    /// The intrinsic of an `external @compiler` member.
    pub(crate) external: Option<CompilerIntrinsic>,
}

/// One module value.
#[derive(Debug)]
pub(crate) struct ValueData<'a> {
    pub(crate) module: usize,
    pub(crate) name: String,
    pub(crate) view: ModuleLet<'a>,
    pub(crate) identity: String,
    pub(crate) qualified: String,
    /// The written type, when this checker supports it.
    pub(crate) ty: Option<Type>,
}

/// One `test`.
#[derive(Debug)]
pub(crate) struct TestData<'a> {
    pub(crate) module: usize,
    /// The decoded name.
    pub(crate) name: String,
    /// The span of the name string, quotes included.
    pub(crate) name_span: ByteSpan,
    pub(crate) view: Test<'a>,
    /// The name the checked IR records: the module and the name, as a
    /// selector writes them.
    pub(crate) qualified: String,
}

/// One callable member of a supported interface.
#[derive(Clone, Debug)]
pub(crate) struct InterfaceMemberData {
    pub(crate) name: String,
    /// The member's name as written.
    pub(crate) span: ByteSpan,
    /// Its type: `Self` is [`Type::SelfType`], the interface's parameters
    /// and then the member's own are [`Type::Binder`]s into `binders`, and
    /// an associated type is its [`Type::Associated`].
    pub(crate) signature: FnType,
    /// The interface's parameters, then the member's own.
    pub(crate) binders: Vec<String>,
    /// The bound of each binder.
    pub(crate) bounds: Vec<Option<BoundData>>,
    /// The receiver slot: the first positional parameter written `Self`.
    pub(crate) receiver: Option<usize>,
    /// The function holding the default body of a default member.
    pub(crate) default: Option<usize>,
    /// The complete row of a `proc` member; `None` for a `fn`.
    pub(crate) row: Option<vibra_ir::Row>,
}

/// One `interface`.
#[derive(Debug)]
pub(crate) struct InterfaceData<'a> {
    pub(crate) module: usize,
    pub(crate) name: String,
    pub(crate) view: InterfaceDecl<'a>,
    /// The checked IR identity, when this checker supports the interface.
    pub(crate) id: Option<TypeId>,
    /// The type parameters, in written order.
    pub(crate) params: Vec<String>,
    /// The bound of each parameter.
    pub(crate) bounds: Vec<Option<BoundData>>,
    /// The associated types, in declaration order, with where each is
    /// declared.
    pub(crate) associated: Vec<(String, ByteSpan)>,
    pub(crate) members: Vec<InterfaceMemberData>,
}

impl InterfaceData<'_> {
    /// The member `name`.
    pub(crate) fn member(&self, name: &str) -> Option<&InterfaceMemberData> {
        self.members.iter().find(|member| member.name == name)
    }

    /// The associated names as they denote themselves inside the
    /// interface.
    pub(crate) fn own_associated(&self) -> Vec<(String, Type)> {
        self.associated
            .iter()
            .map(|(name, _)| (name.clone(), Type::Associated(name.clone())))
            .collect()
    }
}

/// One implementation of a supported interface: an `impl` nested in a type
/// or in an interface.
#[derive(Debug)]
pub(crate) struct ImplData {
    pub(crate) module: usize,
    pub(crate) interface: usize,
    /// The type parameters its key is open in.
    pub(crate) binders: Vec<String>,
    /// The bound of each binder.
    pub(crate) bounds: Vec<Option<BoundData>>,
    pub(crate) receiver: Type,
    /// The interface's positional arguments.
    pub(crate) arguments: Vec<Type>,
    /// The choice of every associated type, in canonical name order.
    pub(crate) choices: Vec<(String, Type)>,
    /// The function supplying each member it supplies.
    pub(crate) members: Vec<(String, usize)>,
    /// The `impl` head: the keyword and the target.
    pub(crate) head: ByteSpan,
    /// Whether dispatch, widening, and bounds may select it: it supplies
    /// every abstract member and overlaps no earlier implementation.
    pub(crate) usable: bool,
}

/// What a written application applies: an interface, a declared type, or
/// the standard library's `Dict` at a key that mentions a type parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoalHead {
    /// An interface type, an interface value type, a bound, or an `impl`
    /// target.
    Interface(usize),
    /// A declared type with parameters.
    Type(usize),
    /// `Dict` at an open key, whose one argument is the key: it MUST
    /// implement `Ordered` under the bounds in scope.
    DictKey,
}

/// A written application of `head` at `arguments`, in `module`. Each
/// argument MUST satisfy the bound of its parameter
/// (`docs/spec/02-type-system.md`, "Generics"), and an open dict key MUST
/// implement `Ordered` ("Dict keys, equality, and order"); both are decided
/// once every implementation is known, with the bounds in scope where the
/// application is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundGoal {
    pub(crate) module: usize,
    pub(crate) head: GoalHead,
    pub(crate) arguments: Vec<Type>,
    /// Where each argument is written.
    pub(crate) spans: Vec<ByteSpan>,
    /// The whole application.
    pub(crate) application: ByteSpan,
}

/// Where an `impl` is nested: in a type, or in an interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Host {
    Type(usize),
    Interface(usize),
}

/// The package and version of a provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackageName {
    pub(crate) name: String,
    pub(crate) version: String,
}

/// Every declaration of the graph with the headers of the supported ones.
#[derive(Debug)]
pub(crate) struct Program<'a> {
    pub(crate) modules: Vec<ModuleData<'a>>,
    pub(crate) types: Vec<TypeData<'a>>,
    pub(crate) functions: Vec<FnData<'a>>,
    pub(crate) values: Vec<ValueData<'a>>,
    pub(crate) tests: Vec<TestData<'a>>,
    pub(crate) interfaces: Vec<InterfaceData<'a>>,
    pub(crate) impls: Vec<ImplData>,
    /// The declaration each prelude name resolves to.
    pub(crate) prelude: BTreeMap<String, Entity>,
    /// The type playing `@str`, when supported.
    pub(crate) str_type: Option<Type>,
    /// The type playing `@bool`, when supported.
    pub(crate) bool_type: Option<Type>,
    /// Notes recording the standard-library declarations that were skipped.
    pub(crate) notes: Vec<String>,
    /// `@tool.unavailable` for each unsupported local declaration in scope.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The declaration bound to each library declaration.
    pub(crate) library: Vec<(Library, Entity)>,
    /// The receiver of every written `impl Ordered`.
    pub(crate) ordered: Vec<Type>,
    /// Every written application of a bounded interface, whose arguments
    /// are checked against the interface's bounds once every implementation
    /// is known.
    pub(crate) goals: RefCell<Vec<BoundGoal>>,
    /// The `@type.suspiciousRow` warnings of the written callable types.
    pub(crate) callable_warnings: super::effects::values::Warnings,
    local_package: PackageName,
    embedded_package: PackageName,
}

/// Why a written type has no checked type here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NoType {
    /// The form is outside this checker's support.
    Unsupported,
    /// A name that resolution already reported, or a fault reported
    /// elsewhere.
    Unresolved,
    /// A fault of the written type, still to be reported; it has no source.
    Fault(Box<Diagnostic>),
}

/// The inputs [`Program::build`] reads beside the parsed modules.
pub(crate) struct BuildInputs<'s> {
    pub(crate) local: PackageName,
    pub(crate) embedded: PackageName,
    /// The local units whose modules are checked, `@` included.
    pub(crate) units: &'s [String],
    /// Each prelude name with the module and name of its declaration.
    pub(crate) prelude: Vec<(String, String, String)>,
    /// The module and name of the declarations playing `@str` and `@bool`.
    pub(crate) str_role: Option<(String, String)>,
    pub(crate) bool_role: Option<(String, String)>,
    /// The module and name of each library declaration.
    pub(crate) library: Vec<(Library, String, String)>,
}

impl<'a> Program<'a> {
    /// Collects every declaration of `documents`, one per source, and reads
    /// the headers this checker supports.
    pub(crate) fn build(
        sources: &[Source],
        documents: &'a [Document],
        inputs: &BuildInputs<'_>,
    ) -> Self {
        let mut program = Self {
            modules: Vec::new(),
            types: Vec::new(),
            functions: Vec::new(),
            values: Vec::new(),
            tests: Vec::new(),
            interfaces: Vec::new(),
            impls: Vec::new(),
            prelude: BTreeMap::new(),
            str_type: None,
            bool_type: None,
            notes: Vec::new(),
            diagnostics: Vec::new(),
            library: Vec::new(),
            ordered: Vec::new(),
            goals: RefCell::new(Vec::new()),
            callable_warnings: super::effects::values::Warnings::default(),
            local_package: inputs.local.clone(),
            embedded_package: inputs.embedded.clone(),
        };
        let mut pending_impls = Vec::new();
        for (source, document) in sources.iter().zip(documents) {
            program.collect_module(source, document, inputs, &mut pending_impls);
        }
        program.imports();
        program.mark_scope(inputs.units);
        for (name, module, declaration) in &inputs.prelude {
            if let Some(entity) = program.top_level(module, declaration) {
                program.prelude.insert(name.clone(), entity);
            }
        }
        program.library = inputs
            .library
            .iter()
            .filter_map(|(library, module, name)| {
                program
                    .top_level(module, name)
                    .map(|entity| (*library, entity))
            })
            .collect();
        program.interface_shapes();
        program.type_kinds(&pending_impls);
        program.type_bounds();
        program.interface_headers();
        program.function_headers();
        program.value_headers();
        program.implementations(pending_impls);
        program.overlaps();
        program.str_type = inputs
            .str_role
            .as_ref()
            .and_then(|(module, name)| program.role_type(module, name));
        program.bool_type = inputs
            .bool_role
            .as_ref()
            .and_then(|(module, name)| program.role_type(module, name));
        program
    }

    fn role_type(&self, module: &str, name: &str) -> Option<Type> {
        match self.top_level(module, name)? {
            Entity::Type(index) => self.types.get(index)?.self_type(),
            _ => None,
        }
    }

    /// The top-level declaration `name` of the module at `path`.
    pub(crate) fn top_level(&self, path: &str, name: &str) -> Option<Entity> {
        let module = self.modules.iter().find(|module| module.path == path)?;
        module
            .scope
            .get(name)
            .copied()
            .filter(|entity| !matches!(entity, Entity::Module(_)))
    }

    fn collect_module(
        &mut self,
        source: &Source,
        document: &'a Document,
        inputs: &BuildInputs<'_>,
        pending_impls: &mut Vec<(Host, Impl<'a>)>,
    ) {
        let module_index = self.modules.len();
        let view = vibra_syntax::module(document);
        self.modules.push(ModuleData {
            path: source.path.clone(),
            source_id: Arc::from(source.source_id.as_str()),
            embedded: source.embedded,
            tests: source.tests,
            package: source.package.clone(),
            in_scope: false,
            view,
            text: document.text(),
            scope: BTreeMap::new(),
        });
        let Some(view) = view else { return };
        let package = if source.embedded {
            &inputs.embedded
        } else {
            source.package.as_ref().unwrap_or(&inputs.local)
        };
        let dotted = source
            .path
            .strip_prefix('@')
            .unwrap_or(&source.path)
            .to_owned();
        let identity = |owner: &str, kind: &str, name: &str| {
            declaration_identity(
                &package.name,
                &package.version,
                &source.path,
                owner,
                kind,
                name,
            )
        };
        for declaration in view.declarations() {
            match declaration {
                Declaration::Use(_) => {}
                Declaration::Type(decl) => {
                    let Some(name) = decl.name() else { continue };
                    let index = self.types.len();
                    let type_name = name.text().to_owned();
                    let params = decl
                        .params()
                        .iter()
                        .map(|param| param.text().to_owned())
                        .collect::<Vec<_>>();
                    let representation = decl.representation().or(decl.visibility());
                    let representation_public = representation
                        .and_then(|section| section.tag())
                        .is_some_and(|tag| tag.text() == "@public");
                    self.types.push(TypeData {
                        module: module_index,
                        name: type_name.clone(),
                        view: decl,
                        identity: identity("", "@type", name.text()),
                        kind: TypeKind::Unsupported,
                        id: None,
                        bounds: vec![None; params.len()],
                        params: params.clone(),
                        representation_public,
                        invariant: false,
                        fallible: false,
                        members: BTreeMap::new(),
                    });
                    self.declare(module_index, &type_name, Entity::Type(index));
                    for member in decl.members() {
                        let Some(member_name) = member.name() else {
                            continue;
                        };
                        let fn_index = self.functions.len();
                        let mut binders = params.clone();
                        binders.extend(
                            member.params().iter().map(|param| param.text().to_owned()),
                        );
                        self.functions.push(FnData {
                            module: module_index,
                            owner: Some(index),
                            implements: None,
                            name: member_name.text().to_owned(),
                            view: member,
                            identity: identity(
                                &type_name,
                                effects::procs::identity_kind(&member),
                                member_name.text(),
                            ),
                            qualified: format!(
                                "{dotted}.{type_name}.{}",
                                member_name.text()
                            ),
                            binders,
                            bounds: Vec::new(),
                            self_type: None,
                            associated: Vec::new(),
                            contract: None,
                            signature: None,
                            external: None,
                        });
                        if let Some(data) = self.types.get_mut(index) {
                            data.members
                                .insert(member_name.text().to_owned(), fn_index);
                        }
                    }
                    for block in decl.impls() {
                        pending_impls.push((Host::Type(index), block));
                    }
                }
                Declaration::Interface(decl) => {
                    let Some(name) = decl.name() else { continue };
                    let index = self.interfaces.len();
                    self.interfaces.push(InterfaceData {
                        module: module_index,
                        name: name.text().to_owned(),
                        view: decl,
                        id: None,
                        params: Vec::new(),
                        bounds: Vec::new(),
                        associated: Vec::new(),
                        members: Vec::new(),
                    });
                    self.declare(module_index, name.text(), Entity::Interface(index));
                    for block in decl.impls() {
                        pending_impls.push((Host::Interface(index), block));
                    }
                }
                Declaration::Fn(callable) => {
                    let Some(name) = callable.name() else {
                        continue;
                    };
                    let index = self.functions.len();
                    self.functions.push(FnData {
                        module: module_index,
                        owner: None,
                        implements: None,
                        name: name.text().to_owned(),
                        view: callable,
                        identity: identity("", "@fn", name.text()),
                        qualified: format!("{dotted}.{}", name.text()),
                        binders: callable
                            .params()
                            .iter()
                            .map(|param| param.text().to_owned())
                            .collect(),
                        bounds: Vec::new(),
                        self_type: None,
                        associated: Vec::new(),
                        contract: None,
                        signature: None,
                        external: None,
                    });
                    self.declare(module_index, name.text(), Entity::Fn(index));
                }
                Declaration::Proc(callable) => {
                    effects::procs::declare(self, module_index, callable);
                }
                Declaration::Let(view) => {
                    let Some(name) = view.name() else { continue };
                    let index = self.values.len();
                    self.values.push(ValueData {
                        module: module_index,
                        name: name.text().to_owned(),
                        view,
                        identity: identity("", "@value", name.text()),
                        qualified: format!("{dotted}.{}", name.text()),
                        ty: None,
                    });
                    self.declare(module_index, name.text(), Entity::Value(index));
                }
                Declaration::Test(view) => {
                    let name =
                        view.construct().contents().iter().find(|child| {
                            child.kind() == vibra_syntax::SyntaxKind::String
                        });
                    let (Some(name), Some(node)) = (view.name_value(), name) else {
                        continue;
                    };
                    let qualified = format!(
                        "{}::{}",
                        source.path,
                        vibra_syntax::canonical_string(&name)
                    );
                    self.tests.push(TestData {
                        module: module_index,
                        name,
                        name_span: node.span(),
                        view,
                        qualified,
                    });
                }
            }
        }
    }

    /// Declares `name` in the scope of `module`, unless it is declared there.
    pub(crate) fn declare(&mut self, module: usize, name: &str, entity: Entity) {
        if let Some(data) = self.modules.get_mut(module) {
            data.scope.entry(name.to_owned()).or_insert(entity);
        }
    }

    /// Binds each import's local name to the module or declaration its
    /// locator names.
    fn imports(&mut self) {
        let mut bindings = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            let Some(view) = module.view else { continue };
            for import in view.uses() {
                let (Some(locator), Some(name)) = (import.locator(), import.name())
                else {
                    continue;
                };
                let target = locator.text();
                let entity = if let Some(found) =
                    self.modules.iter().position(|module| module.path == target)
                {
                    Some(Entity::Module(found))
                } else {
                    target.rsplit_once('.').and_then(|(module, declaration)| {
                        self.top_level(module, declaration)
                    })
                };
                if let Some(entity) = entity {
                    bindings.push((index, name.to_owned(), entity));
                }
            }
        }
        for (module, name, entity) in bindings {
            self.declare(module, &name, entity);
        }
    }

    /// Marks the embedded modules, the modules of `units` (each a unit or a
    /// module path), and every module they import, transitively, as checked.
    fn mark_scope(&mut self, units: &[String]) {
        let mut pending = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            let unit = module.path.split('.').next().unwrap_or(&module.path);
            if module.embedded
                || units
                    .iter()
                    .any(|selected| *selected == unit || *selected == module.path)
            {
                pending.push(index);
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(index) = pending.pop() {
            if !seen.insert(index) {
                continue;
            }
            let Some(module) = self.modules.get(index) else {
                continue;
            };
            for entity in module.scope.values() {
                let target = match *entity {
                    Entity::Module(target) => Some(target),
                    Entity::Type(i) => self.types.get(i).map(|data| data.module),
                    Entity::Interface(i) => {
                        self.interfaces.get(i).map(|data| data.module)
                    }
                    Entity::Fn(i) => self.functions.get(i).map(|data| data.module),
                    Entity::Value(i) => self.values.get(i).map(|data| data.module),
                };
                if let Some(target) = target {
                    pending.push(target);
                }
            }
        }
        for index in seen {
            if let Some(module) = self.modules.get_mut(index) {
                module.in_scope = true;
            }
        }
    }

    /// Reports an unsupported declaration of a local module in scope, or
    /// records a note for one of the standard library.
    pub(crate) fn unsupported(&mut self, module: usize, span: ByteSpan, what: &str) {
        let Some(data) = self.modules.get(module) else {
            return;
        };
        if !data.in_scope {
            return;
        }
        if data.embedded {
            self.notes.push(format!(
                "{} {what} is skipped: this checker does not support it yet",
                data.path
            ));
        } else {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                span,
                format!("{what} is not supported by this checker yet"),
            )
            .with_source_id(data.source_id.as_ref());
            self.diagnostics.push(diagnostic);
        }
    }

    /// Looks `name` up in the scope of `module`, then in the prelude.
    pub(crate) fn lookup(&self, module: usize, name: &str) -> Option<Entity> {
        self.modules
            .get(module)
            .and_then(|data| data.scope.get(name).copied())
            .or_else(|| self.prelude.get(name).copied())
    }

    /// Reports a fault found in a declaration of `module`: as a diagnostic in
    /// a local module in scope, as a note in the standard library.
    pub(crate) fn fault(&mut self, module: usize, diagnostic: Diagnostic) {
        let Some(data) = self.modules.get(module) else {
            return;
        };
        if !data.in_scope {
            return;
        }
        if data.embedded {
            self.notes.push(format!(
                "{} has a declaration this checker rejects: {}",
                data.path,
                diagnostic.message()
            ));
        } else {
            let diagnostic = diagnostic.with_source_id(data.source_id.as_ref());
            self.diagnostics.push(diagnostic);
        }
    }

    /// Reports why a written type of the declaration `what` has no checked
    /// type.
    fn no_type(&mut self, module: usize, span: ByteSpan, what: &str, why: NoType) {
        match why {
            NoType::Unsupported => self.unsupported(module, span, what),
            NoType::Unresolved => {}
            NoType::Fault(diagnostic) => self.fault(module, *diagnostic),
        }
    }

    /// The program index of the type bound to `library`.
    pub(crate) fn library_index(&self, library: Library) -> Option<usize> {
        self.library
            .iter()
            .find_map(|(binding, entity)| match entity {
                Entity::Type(index) if *binding == library => Some(*index),
                _ => None,
            })
    }

    /// The library declaration the type `id` plays, if any.
    pub(crate) fn library_of(&self, id: &TypeId) -> Option<Library> {
        self.library
            .iter()
            .find_map(|(binding, entity)| match entity {
                Entity::Type(index)
                    if self.types.get(*index).and_then(|data| data.id.as_ref())
                        == Some(id) =>
                {
                    Some(*binding)
                }
                _ => None,
            })
    }

    /// The program index of the declared type `id`.
    pub(crate) fn type_index_of(&self, id: &TypeId) -> Option<usize> {
        self.types
            .iter()
            .position(|data| data.id.as_ref() == Some(id))
    }

    /// The library type `library` applied to `arguments`, when supported.
    pub(crate) fn library_type(
        &self,
        library: Library,
        arguments: Vec<Type>,
    ) -> Option<Type> {
        let data = self.types.get(self.library_index(library)?)?;
        if data.params.len() != arguments.len() {
            return None;
        }
        data.applied(arguments)
    }

    /// Why `key` cannot key a dict, when it cannot: a type that does not
    /// implement `Ordered`, written or structurally, is
    /// `@type.invalidDictKey`. A callable anywhere in its structure
    /// implements none, open or closed.
    pub(crate) fn key_fault(&self, key: &Type) -> Option<DiagnosticCode> {
        if holds_callable(key) {
            return Some(DiagnosticCode::TypeInvalidDictKey);
        }
        let mut pending = vec![key];
        while let Some(ty) = pending.pop() {
            match ty {
                Type::Atom | Type::AtomSingleton(_) => {}
                Type::Tuple(_) | Type::Record(_) => pending.extend(ty.components()),
                // An implementation in a generic type covers every
                // application of it, each of which meets the type's bounds.
                Type::Declared(id, _)
                    if self.ordered.iter().any(|receiver| {
                        matches!(receiver, Type::Declared(ordered, _) if ordered == id)
                    }) => {}
                other if self.ordered.contains(other) => {}
                _ => return Some(DiagnosticCode::TypeInvalidDictKey),
            }
        }
        None
    }

    /// The checked type of a written type expression, read in `module` in
    /// `scope`. Its depth is measured first, without recursing, so every
    /// later walk over it is bounded.
    pub(crate) fn written(
        &self,
        module: usize,
        written: TypeExpr<'_>,
        scope: Scope<'_>,
    ) -> Result<Type, NoType> {
        if written_depth(written) > Type::MAX_DEPTH {
            return Err(depth_fault(written.span()));
        }
        let ty = self.written_at(module, written, scope)?;
        if ty.depth() > Type::MAX_DEPTH {
            return Err(depth_fault(written.span()));
        }
        Ok(ty)
    }

    /// The checked type of a written type expression whose depth
    /// [`Self::written`] has measured.
    pub(crate) fn written_at(
        &self,
        module: usize,
        written: TypeExpr<'_>,
        scope: Scope<'_>,
    ) -> Result<Type, NoType> {
        match written.form().ok_or(NoType::Unresolved)? {
            TypeForm::Path {
                path,
                positional,
                labelled,
            } => {
                let segments = path.segments();
                let (first, rest) = segments.split_first().ok_or(NoType::Unresolved)?;
                if rest.is_empty() && positional.is_empty() && labelled.is_empty() {
                    if first.text() == "Self" {
                        return scope.self_type.cloned().ok_or(NoType::Unsupported);
                    }
                    if let Some(at) = scope
                        .binders
                        .iter()
                        .position(|binder| binder == first.text())
                    {
                        return Ok(Type::Binder(at));
                    }
                    if let Some((_, ty)) = scope
                        .associated
                        .iter()
                        .find(|(name, _)| name == first.text())
                    {
                        return Ok(ty.clone());
                    }
                    if first.text() == "Any" && self.lookup(module, "Any").is_none() {
                        return Ok(Type::Any);
                    }
                }
                let mut entity = self
                    .lookup(module, first.text())
                    .ok_or(NoType::Unresolved)?;
                for part in rest {
                    entity = match entity {
                        Entity::Module(target) => self
                            .modules
                            .get(target)
                            .and_then(|data| data.scope.get(part.text()).copied())
                            .ok_or(NoType::Unresolved)?,
                        _ => return Err(NoType::Unresolved),
                    };
                }
                let index = match entity {
                    Entity::Type(index) => index,
                    Entity::Interface(index) => {
                        let (interface, arguments, choices) = self
                            .interface_application(
                                module,
                                index,
                                written,
                                &positional,
                                &labelled,
                                scope,
                                true,
                            )?;
                        let id = self
                            .interfaces
                            .get(interface)
                            .and_then(|data| data.id.clone())
                            .ok_or(NoType::Unsupported)?;
                        let choices =
                            NamedTypes::new(choices).map_err(|_| NoType::Unresolved)?;
                        return Ok(Type::Interface(id, arguments, choices));
                    }
                    _ => return Err(NoType::Unresolved),
                };
                let data = self.types.get(index).ok_or(NoType::Unresolved)?;
                let written_count = positional.len() + labelled.len();
                if !labelled.is_empty() || positional.len() != data.params.len() {
                    return Err(NoType::Fault(Box::new(Diagnostic::new(
                        DiagnosticCode::TypeTypeArgumentMismatch,
                        written.span(),
                        format!(
                            "`{}` takes {} positional type argument(s), but {written_count} are written",
                            data.name,
                            data.params.len()
                        ),
                    ))));
                }
                match data.kind {
                    TypeKind::Unsupported => return Err(NoType::Unsupported),
                    TypeKind::Faulty => return Err(NoType::Unresolved),
                    _ => {}
                }
                let arguments = positional
                    .iter()
                    .map(|argument| self.written_at(module, *argument, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                if self.library_index(Library::Dict) == Some(index)
                    && let (Some(key), Some(key_written)) =
                        (arguments.first(), positional.first())
                {
                    // A key that mentions a type parameter implements
                    // `Ordered` exactly when the bounds in scope show it, which
                    // is decided where the type is written, once every
                    // implementation is known.
                    let code = if !key.is_open() {
                        self.key_fault(key)
                    } else {
                        holds_callable(key)
                            .then_some(DiagnosticCode::TypeInvalidDictKey)
                    };
                    if let Some(code) = code {
                        return Err(NoType::Fault(Box::new(key_diagnostic(
                            code,
                            key_written.span(),
                            key,
                        ))));
                    }
                    if key.is_open() {
                        self.record_goal(
                            module,
                            GoalHead::DictKey,
                            std::slice::from_ref(key),
                            vec![key_written.span()],
                            written.span(),
                        );
                    }
                }
                self.record_goal(
                    module,
                    GoalHead::Type(index),
                    &arguments,
                    positional.iter().map(TypeExpr::span).collect(),
                    written.span(),
                );
                data.applied(arguments).ok_or(NoType::Unsupported)
            }
            TypeForm::Atom(atom) => Ok(Type::AtomSingleton(
                atom.text()
                    .strip_prefix('@')
                    .unwrap_or(atom.text())
                    .to_owned(),
            )),
            TypeForm::Tuple(components) => Ok(Type::Tuple(
                components
                    .into_iter()
                    .map(|component| self.written_at(module, component, scope))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            TypeForm::Record(fields) => {
                let mut seen: BTreeMap<&str, ByteSpan> = BTreeMap::new();
                for (name, _) in &fields {
                    if let Some(first) = seen.get(name.text()) {
                        return Err(NoType::Fault(Box::new(
                            Diagnostic::new(
                                DiagnosticCode::NameRedeclaration,
                                name.span(),
                                format!("the field `{}` is written twice", name.text()),
                            )
                            .with_related(*first, "the earlier field"),
                        )));
                    }
                    seen.insert(name.text(), name.span());
                }
                let fields = fields
                    .into_iter()
                    .map(|(name, field)| {
                        Ok((
                            name.text().to_owned(),
                            self.written_at(module, field, scope)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, NoType>>()?;
                Type::record(fields).map_err(|_| NoType::Unsupported)
            }
            TypeForm::Fn { inputs, result } => {
                if let Some(fault) = repeated_label(&inputs) {
                    return Err(fault);
                }
                let parameters = inputs
                    .positional()
                    .into_iter()
                    .map(|input| self.written_at(module, input, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                let labelled = inputs
                    .labelled()
                    .into_iter()
                    .map(|(name, input)| {
                        Ok((
                            name.text().to_owned(),
                            self.written_at(module, input, scope)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, NoType>>()?;
                let labels =
                    NamedTypes::new(labelled).map_err(|_| NoType::Unresolved)?;
                let result = self.written_at(module, result, scope)?;
                Ok(Type::function(FnType {
                    parameters,
                    labels,
                    result,
                }))
            }
            TypeForm::Proc { .. } | TypeForm::Intent { .. } => {
                effects::values::written_type(self, module, written, scope)
            }
        }
    }

    /// Decides the shape of every type: first which bodies this checker can
    /// read, then each body, until no type turns out unsupported.
    fn type_kinds(&mut self, pending_impls: &[(Host, Impl<'a>)]) {
        let mut invalid = Vec::new();
        for index in 0..self.types.len() {
            let Some(data) = self.types.get(index) else {
                continue;
            };
            let embedded = self
                .modules
                .get(data.module)
                .is_some_and(|module| module.embedded);
            // A `where` on a wrapper, declared tuple, or declared record
            // states the type's invariant, whose predicate the bodies pass
            // checks. A `where` on a variant is a fault at the `where`, and
            // the variant is read without it.
            let invariant = data.view.invariant();
            let of_body = matches!(data.view.body(), Some(TypeBody::Of(_)));
            if let Some(section) = invariant
                && matches!(data.view.body(), Some(TypeBody::Cases(_)))
            {
                invalid.push((
                    data.module,
                    Diagnostic::new(
                        DiagnosticCode::TypeInvalidInvariant,
                        where_span(section),
                        "a variant type has no representation for a `where` to \
                         state a predicate over; check its cases in a function",
                    ),
                ));
            }
            let stated = invariant.filter(|_| of_body);
            let plain = bounds_are_any(data.view.given());
            let kind = match data.view.body() {
                Some(TypeBody::Intrinsic(tag))
                    if plain && embedded && invariant.is_none() =>
                {
                    if tag.text() == "@list" && data.params.len() == 1 {
                        TypeKind::List
                    } else if data.params.is_empty() {
                        scalar(tag.text())
                            .map_or(TypeKind::Unsupported, TypeKind::Scalar)
                    } else {
                        TypeKind::Unsupported
                    }
                }
                Some(TypeBody::Cases(_) | TypeBody::Of(_)) => TypeKind::Pending,
                _ => TypeKind::Unsupported,
            };
            let id = if kind == TypeKind::Pending {
                let path = format!(
                    "{}.{}",
                    self.modules
                        .get(data.module)
                        .map(|module| module.path.trim_start_matches('@'))
                        .unwrap_or_default(),
                    data.name
                );
                TypeId::new(path, data.identity.clone()).ok()
            } else {
                None
            };
            let kind = if kind == TypeKind::Pending && id.is_none() {
                TypeKind::Unsupported
            } else {
                kind
            };
            if let Some(data) = self.types.get_mut(index) {
                data.kind = kind;
                data.id = id;
                data.invariant = stated.is_some();
                data.fallible =
                    stated.is_some_and(|section| section.consequence().is_some());
            }
        }
        for (module, diagnostic) in invalid {
            self.fault(module, diagnostic);
        }
        self.ordered = self.ordered_receivers(pending_impls);
        let mut faults = Vec::new();
        loop {
            let mut changed = false;
            let mut decided = Vec::new();
            for (index, data) in self.types.iter().enumerate() {
                if matches!(
                    data.kind,
                    TypeKind::Pending
                        | TypeKind::Wrapper(_)
                        | TypeKind::Tuple(_)
                        | TypeKind::Record(_)
                        | TypeKind::Variant(_)
                ) {
                    decided.push((index, self.body_kind(index)));
                }
            }
            for (index, kind) in decided {
                let Some(data) = self.types.get_mut(index) else {
                    continue;
                };
                match kind {
                    Ok(kind) => data.kind = kind,
                    Err(why) => {
                        changed = true;
                        data.id = None;
                        data.kind = match why {
                            NoType::Fault(diagnostic) => {
                                faults.push((data.module, *diagnostic));
                                TypeKind::Faulty
                            }
                            NoType::Unsupported | NoType::Unresolved => {
                                TypeKind::Unsupported
                            }
                        };
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for (module, diagnostic) in faults {
            self.fault(module, diagnostic);
        }
        super::nominal::infinite_sizes(self);
        for index in 0..self.types.len() {
            let Some(data) = self.types.get(index) else {
                continue;
            };
            if data.kind != TypeKind::Unsupported {
                continue;
            }
            let module = data.module;
            let span = data.view.construct().span();
            let what = format!("the type `{}`", data.name);
            self.unsupported(module, span, &what);
        }
    }

    /// Reads the bound of every parameter of every type this checker reads,
    /// in the type's own scope. A bound that does not read is reported once
    /// and left `Any`, so the fault does not cascade.
    fn type_bounds(&mut self) {
        let mut read = Vec::new();
        for (index, data) in self.types.iter().enumerate() {
            if data.view.given().is_none()
                || matches!(data.kind, TypeKind::Unsupported | TypeKind::Faulty)
            {
                continue;
            }
            let self_type = data.self_type();
            let scope = Scope {
                self_type: self_type.as_ref(),
                binders: &data.params,
                associated: &[],
            };
            let bounds = self.read_bounds(
                data.module,
                data.view.given(),
                &data.params,
                0,
                scope,
            );
            read.push((index, bounds));
        }
        for (index, bounds) in read {
            let Some(data) = self.types.get(index) else {
                continue;
            };
            match bounds {
                Ok(bounds) => {
                    if let Some(data) = self.types.get_mut(index) {
                        data.bounds = bounds;
                    }
                }
                Err(why) => {
                    let module = data.module;
                    let span = data.view.construct().span();
                    let what = format!("the bounds of `{}`", data.name);
                    self.no_type(module, span, &what, why);
                }
            }
        }
    }

    /// The receiver of every `impl Ordered` written in a type: the type
    /// applied to its own parameters, which the implementation passes
    /// through with the type's bounds.
    fn ordered_receivers(&self, pending_impls: &[(Host, Impl<'a>)]) -> Vec<Type> {
        let Some(ordered) = self.library.iter().find_map(|(binding, entity)| {
            (*binding == Library::Ordered).then_some(*entity)
        }) else {
            return Vec::new();
        };
        let mut receivers = Vec::new();
        for (host, block) in pending_impls {
            let Host::Type(owner) = host else { continue };
            let Some(data) = self.types.get(*owner) else {
                continue;
            };
            let target = block.target().and_then(|target| match target.form() {
                Some(TypeForm::Path {
                    path,
                    positional,
                    labelled,
                }) if positional.is_empty() && labelled.is_empty() => {
                    match path.segments().as_slice() {
                        [name] => self.lookup(data.module, name.text()),
                        _ => None,
                    }
                }
                _ => None,
            });
            if target == Some(ordered)
                && let Some(receiver) = data.self_type()
            {
                receivers.push(receiver);
            }
        }
        receivers
    }

    /// The shape of the body of the type `index`.
    fn body_kind(&self, index: usize) -> Result<TypeKind, NoType> {
        let data = self.types.get(index).ok_or(NoType::Unresolved)?;
        let module = data.module;
        let self_type = data.self_type();
        let scope = Scope {
            self_type: self_type.as_ref(),
            binders: &data.params,
            associated: &[],
        };
        match data.view.body() {
            Some(TypeBody::Cases(cases)) => {
                let mut read = Vec::new();
                for case in cases {
                    let name = case.name().ok_or(NoType::Unresolved)?.text().to_owned();
                    let fields = match case.fields() {
                        None => None,
                        Some(fields) => Some(
                            fields
                                .into_iter()
                                .map(|(field, written)| {
                                    Ok((
                                        field.text().to_owned(),
                                        self.written(module, written, scope)?,
                                    ))
                                })
                                .collect::<Result<Vec<_>, NoType>>()?,
                        ),
                    };
                    read.push(CaseData { name, fields });
                }
                if read.is_empty() {
                    return Err(NoType::Unsupported);
                }
                Ok(TypeKind::Variant(read))
            }
            Some(TypeBody::Of(written)) => {
                if written_depth(written) > Type::MAX_DEPTH {
                    return Err(depth_fault(written.span()));
                }
                match written.form() {
                    Some(TypeForm::Tuple(components)) => Ok(TypeKind::Tuple(
                        components
                            .into_iter()
                            .map(|component| self.written(module, component, scope))
                            .collect::<Result<Vec<_>, _>>()?,
                    )),
                    Some(TypeForm::Record(fields)) => Ok(TypeKind::Record(
                        fields
                            .into_iter()
                            .map(|(field, written)| {
                                Ok((
                                    field.text().to_owned(),
                                    self.written(module, written, scope)?,
                                ))
                            })
                            .collect::<Result<Vec<_>, NoType>>()?,
                    )),
                    _ => Ok(TypeKind::Wrapper(self.written(module, written, scope)?)),
                }
            }
            _ => Err(NoType::Unsupported),
        }
    }

    /// The identity of an interface: its anchor in its package.
    pub(crate) fn interface_identity(&self, index: usize) -> Option<String> {
        let data = self.interfaces.get(index)?;
        let module = self.modules.get(data.module)?;
        Some(self.identity_of(
            &module.path,
            module.embedded,
            "",
            "@interface",
            &data.name,
        ))
    }

    /// The identity of the `impl` block nested in `owner` in the module
    /// `module` whose target is written `target`: see
    /// [`vibra_resolve::implementation_identity`].
    pub(crate) fn implementation_identity(
        &self,
        module: usize,
        owner: &str,
        target: &str,
    ) -> String {
        let Some(data) = self.modules.get(module) else {
            return String::new();
        };
        let package = if data.embedded {
            &self.embedded_package
        } else {
            data.package.as_ref().unwrap_or(&self.local_package)
        };
        vibra_resolve::implementation_identity(
            &package.name,
            &package.version,
            &data.path,
            owner,
            target,
        )
        .as_str()
        .to_owned()
    }

    pub(crate) fn identity_of(
        &self,
        module: &str,
        embedded: bool,
        owner: &str,
        kind: &str,
        name: &str,
    ) -> String {
        let package = if embedded {
            &self.embedded_package
        } else {
            self.modules
                .iter()
                .find(|data| !data.embedded && data.path == module)
                .and_then(|data| data.package.as_ref())
                .unwrap_or(&self.local_package)
        };
        declaration_identity(&package.name, &package.version, module, owner, kind, name)
    }

    /// The type of a callable header with written supported types, read in
    /// `scope`. Its parameter patterns are checked with its body.
    pub(crate) fn callable_type(
        &self,
        module: usize,
        view: &Callable<'_>,
        scope: Scope<'_>,
    ) -> Result<FnType, NoType> {
        let mut parameters = Vec::new();
        let mut deepest: Option<ByteSpan> = None;
        for (_, written) in view.takes() {
            let span = written.span();
            let ty = self.written(module, written, scope)?;
            if deepest.is_none() && ty.depth() >= Type::MAX_DEPTH {
                deepest = Some(span);
            }
            parameters.push(ty);
        }
        let mut labelled = Vec::new();
        for (name, written, _) in view.labelled() {
            let span = written.span();
            let ty = self.written(module, written, scope)?;
            if deepest.is_none() && ty.depth() >= Type::MAX_DEPTH {
                deepest = Some(span);
            }
            labelled.push((name.text().to_owned(), ty));
        }
        // A label written twice is a repeated lexical binding, which
        // resolution reports.
        let labels = NamedTypes::new(labelled).map_err(|_| NoType::Unresolved)?;
        let (_, written) = view.result().ok_or(NoType::Unresolved)?;
        let span = written.span();
        let result = self.written(module, written, scope)?;
        if deepest.is_none() && result.depth() >= Type::MAX_DEPTH {
            deepest = Some(span);
        }
        // The callable's own type is one level more than its deepest
        // parameter or result, and is a type of the program too.
        if let Some(span) = deepest {
            return Err(depth_fault(span));
        }
        Ok(FnType {
            parameters,
            labels,
            result,
        })
    }

    /// The type parameters `binders` of a `fn` and their bounds `bounds`,
    /// with one more type parameter after them for each associated type a
    /// bound leaves open: the bound's label names it, so each use takes it
    /// from the implementation the bound selects, and inside the `fn` it
    /// equals only itself. It is named after the bounded parameter and the
    /// associated type, `TItem` for `T` and `Item`, with a number after a
    /// name already taken.
    pub(crate) fn open_choices(
        &self,
        binders: &[String],
        mut bounds: Vec<Option<BoundData>>,
    ) -> (Vec<String>, Vec<Option<BoundData>>) {
        let mut binders = binders.to_vec();
        bounds.resize(binders.len(), None);
        for at in 0..binders.len() {
            let Some(Some(bound)) = bounds.get(at).cloned() else {
                continue;
            };
            let Some(interface) = self.interfaces.get(bound.interface) else {
                continue;
            };
            let mut choices = bound.choices.clone();
            for (name, _) in &interface.associated {
                if choices.iter().any(|(chosen, _)| chosen == name) {
                    continue;
                }
                let base =
                    format!("{}{name}", binders.get(at).map_or("", String::as_str));
                let mut fresh = base.clone();
                let mut suffix = 2;
                while binders.contains(&fresh) {
                    fresh = format!("{base}{suffix}");
                    suffix += 1;
                }
                choices.push((name.clone(), Type::Binder(binders.len())));
                binders.push(fresh);
                bounds.push(None);
            }
            choices.sort_by(|left, right| left.0.cmp(&right.0));
            if let Some(Some(slot)) = bounds.get_mut(at) {
                slot.choices = choices;
            }
        }
        (binders, bounds)
    }

    /// Reads the header of every `fn`.
    fn function_headers(&mut self) {
        let mut results = Vec::new();
        for (index, data) in self.functions.iter().enumerate() {
            if data.contract.is_some() || data.implements.is_some() {
                continue;
            }
            let owner = data.owner.and_then(|owner| self.types.get(owner));
            let owner_supported = data.owner.is_none()
                || owner.is_some_and(|owner| owner.self_type().is_some());
            if !owner_supported {
                results.push((index, Err(NoType::Unresolved), None, None));
                continue;
            }
            let self_type = owner.and_then(TypeData::self_type);
            let external = data
                .view
                .symbol()
                .and_then(|symbol| symbol.value())
                .and_then(|symbol| CompilerIntrinsic::from_symbol(&symbol));
            let scope = Scope {
                self_type: self_type.as_ref(),
                binders: &data.binders,
                associated: &[],
            };
            let own = data.view.params().len();
            // A guard is read with the body; a row and a procedure are read
            // by their hook.
            let header = if let Some(header) = effects::procs::header(self, data, scope)
            {
                header
            } else if external.is_none() && data.view.body().is_none() {
                Err(NoType::Unsupported)
            } else {
                self.read_bounds(
                    data.module,
                    data.view.given(),
                    &data.binders,
                    data.binders.len().saturating_sub(own),
                    scope,
                )
                .and_then(|mut bounds| {
                    // A member sees its owner's parameters with their bounds.
                    if let Some(owner) = owner {
                        for (slot, bound) in bounds.iter_mut().zip(&owner.bounds) {
                            slot.clone_from(bound);
                        }
                    }
                    Ok((self.callable_type(data.module, &data.view, scope)?, bounds))
                })
            };
            results.push((index, header, external, self_type));
        }
        for (index, header, external, self_type) in results {
            let Some(data) = self.functions.get(index) else {
                continue;
            };
            let module = data.module;
            let span = data.view.construct().span();
            let what = effects::procs::describe(data);
            match header {
                Ok((signature, bounds)) => {
                    let (binders, bounds) = self.open_choices(&data.binders, bounds);
                    if let Some(data) = self.functions.get_mut(index) {
                        data.signature = Some(signature);
                        data.external = external;
                        data.binders = binders;
                        data.bounds = bounds;
                        data.self_type = self_type;
                    }
                }
                Err(why) => self.no_type(module, span, &what, why),
            }
        }
    }

    /// Reads the written type of every module value.
    fn value_headers(&mut self) {
        let mut results = Vec::new();
        for (index, data) in self.values.iter().enumerate() {
            let ty = data
                .view
                .written_type()
                .map_or(Err(NoType::Unresolved), |written| {
                    self.written(data.module, written, Scope::default())
                });
            results.push((index, ty));
        }
        for (index, ty) in results {
            let Some(data) = self.values.get(index) else {
                continue;
            };
            let module = data.module;
            let span = data.view.construct().span();
            let what = format!("the module value `{}`", data.name);
            match ty {
                Ok(ty) => {
                    if let Some(data) = self.values.get_mut(index) {
                        data.ty = Some(ty);
                    }
                }
                Err(why) => self.no_type(module, span, &what, why),
            }
        }
    }

    /// Whether the function `index` is supported and lowered.
    pub(crate) fn supported_fn(&self, index: usize) -> bool {
        self.functions.get(index).is_some_and(|data| {
            data.signature.is_some()
                && self
                    .modules
                    .get(data.module)
                    .is_some_and(|module| module.in_scope)
        })
    }

    /// Whether the module value `index` is supported and lowered.
    pub(crate) fn supported_value(&self, index: usize) -> bool {
        self.values.get(index).is_some_and(|data| {
            data.ty.is_some()
                && self
                    .modules
                    .get(data.module)
                    .is_some_and(|module| module.in_scope)
        })
    }

    /// The type and case of the module value `index` when its initializer is
    /// a constant construction of a fieldless case, directly or through the
    /// names of other such module values.
    pub(crate) fn constant_case(&self, index: usize) -> Option<(usize, String)> {
        let mut current = index;
        for _ in 0..=self.values.len() {
            let data = self.values.get(current)?;
            let expr = data.view.value()?;
            if !expr.prefixes().is_empty() {
                return None;
            }
            let ExprForm::Postfix(postfix) = expr.form() else {
                return None;
            };
            let Some(Primary::Name(first)) = postfix.primary() else {
                return None;
            };
            let mut names = vec![first.text()];
            let mut applied = false;
            for suffix in postfix.suffixes() {
                match suffix {
                    Suffix::Field(name) if !applied => names.push(name.text()),
                    Suffix::Call(args) if !applied && args.is_empty() => applied = true,
                    Suffix::Field(_) | Suffix::Call(_) => return None,
                }
            }
            let (first, rest) = names.split_first()?;
            let mut entity = self.lookup(data.module, first)?;
            let mut consumed = 0;
            for name in rest {
                match entity {
                    Entity::Module(module) => {
                        entity = self.modules.get(module)?.scope.get(*name).copied()?;
                        consumed += 1;
                    }
                    _ => break,
                }
            }
            let remaining = rest.get(consumed..).unwrap_or_default();
            match (entity, remaining, applied) {
                (Entity::Type(ty), [case], true) => {
                    let data = self.types.get(ty)?;
                    let fieldless = data.params.is_empty()
                        && data.case(case).is_some_and(|found| found.fields.is_none());
                    return fieldless.then(|| (ty, (*case).to_owned()));
                }
                (Entity::Value(next), [], false) => current = next,
                _ => return None,
            }
        }
        None
    }

    /// The literal leaf the initializer of the module value `index` is,
    /// directly or through the names of other module values.
    pub(crate) fn constant_literal(&self, index: usize) -> Option<&'a CstNode> {
        let mut current = index;
        for _ in 0..=self.values.len() {
            let data = self.values.get(current)?;
            let expr = data.view.value()?;
            if !expr.prefixes().is_empty() {
                return None;
            }
            let ExprForm::Postfix(postfix) = expr.form() else {
                return None;
            };
            let suffixes = postfix.suffixes();
            let first = match postfix.primary()? {
                Primary::Literal(node) => return suffixes.is_empty().then_some(node),
                Primary::Name(first) => first,
                _ => return None,
            };
            let mut entity = self.lookup(data.module, first.text())?;
            for suffix in &suffixes {
                let (Suffix::Field(name), Entity::Module(module)) = (suffix, entity)
                else {
                    return None;
                };
                entity = self.modules.get(module)?.scope.get(name.text()).copied()?;
            }
            let Entity::Value(next) = entity else {
                return None;
            };
            current = next;
        }
        None
    }
}

/// The span of a `where` section: its keyword to the end of its last
/// operand.
pub(crate) fn where_span(section: Where<'_>) -> ByteSpan {
    let node = section.node();
    let start = node
        .children()
        .first()
        .map_or(node.span().start(), |keyword| keyword.span().start());
    let end = section
        .consequence()
        .or_else(|| section.predicate())
        .map_or(node.span().end(), |operand| {
            super::expr::expr_span(&operand).end()
        });
    ByteSpan::new(start, end)
}

/// Whether every entry of a `given` section, if any, is the bound `Any`.
fn bounds_are_any(given: Option<Given<'_>>) -> bool {
    given.is_none_or(|given| {
        given.entries().iter().all(|(_, bound)| {
            matches!(
                bound.form(),
                Some(TypeForm::Path { path, positional, labelled })
                    if path.dotted() == "Any" && positional.is_empty() && labelled.is_empty()
            )
        })
    })
}

/// The compiler-owned scalar type of an intrinsic tag.
fn scalar(tag: &str) -> Option<Type> {
    Some(match tag.strip_prefix('@')? {
        "void" => Type::Void,
        "never" => Type::Never,
        "atom" => Type::Atom,
        "char" => Type::Char,
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "f32" => Type::F32,
        "f64" => Type::F64,
        _ => return None,
    })
}

/// The number of levels a written type nests, measured as the type
/// chapter measures a type and without recursing.
pub(crate) fn written_depth(written: TypeExpr<'_>) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(written, 1_usize)];
    while let Some((written, depth)) = pending.pop() {
        deepest = deepest.max(depth);
        let below = depth.saturating_add(1);
        let children = match written.form() {
            Some(TypeForm::Path {
                positional,
                labelled,
                ..
            }) => positional
                .into_iter()
                .chain(labelled.into_iter().map(|(_, ty)| ty))
                .collect(),
            Some(TypeForm::Tuple(components)) => components,
            Some(TypeForm::Record(fields)) => {
                fields.into_iter().map(|(_, ty)| ty).collect()
            }
            Some(
                TypeForm::Fn { inputs, result } | TypeForm::Proc { inputs, result, .. },
            ) => {
                let mut children = inputs.positional();
                children.extend(inputs.labelled().into_iter().map(|(_, ty)| ty));
                children.push(result);
                children
            }
            Some(TypeForm::Intent { result, .. }) => vec![result],
            Some(TypeForm::Atom(_)) | None => Vec::new(),
        };
        pending.extend(children.into_iter().map(|child| (child, below)));
    }
    deepest
}

/// `@type.depthExceeded` at `span`.
pub(crate) fn depth_fault(span: ByteSpan) -> NoType {
    NoType::Fault(Box::new(Diagnostic::new(
        DiagnosticCode::TypeDepthExceeded,
        span,
        format!("this type nests more than {} levels", Type::MAX_DEPTH),
    )))
}

/// The fault of a written `Fn` or `Proc` type whose inputs repeat a label:
/// `@type.typeArgumentMismatch` at the second occurrence
/// (`docs/spec/02-type-system.md`, "Callable types").
pub(crate) fn repeated_label(inputs: &Inputs<'_>) -> Option<NoType> {
    let labelled = inputs.labelled();
    let mut seen = BTreeSet::new();
    let (name, _) = labelled
        .iter()
        .find(|(name, _)| !seen.insert(name.text()))?;
    Some(NoType::Fault(Box::new(Diagnostic::new(
        DiagnosticCode::TypeTypeArgumentMismatch,
        name.span(),
        format!(
            "the label `{}` is written twice; each label of a callable type names \
             one input",
            name.text()
        ),
    ))))
}

/// The diagnostic `code` of [`Program::key_fault`] for `key` at `span`.
pub(crate) fn key_diagnostic(
    code: DiagnosticCode,
    span: ByteSpan,
    key: &Type,
) -> Diagnostic {
    let message = if holds_callable(key) {
        format!(
            "`{key}` holds a callable, which implements no `Ordered`, so it keys no dict"
        )
    } else {
        format!("`{key}` does not implement `Ordered`, so it keys no dict")
    };
    Diagnostic::new(code, span, message)
}

/// Whether a callable type occurs anywhere in the structure of `ty`.
fn holds_callable(ty: &Type) -> bool {
    let mut callable = false;
    ty.visit(&mut |inner| {
        callable |= matches!(inner, Type::Fn(_) | Type::Proc(_) | Type::Intent(_));
    });
    callable
}
