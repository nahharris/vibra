//! Declarations: every module of the checked graph, its names, and the
//! headers of the declarations this checker supports.
//!
//! `docs/spec/02-type-system.md`, "Declared types", "The prelude", and
//! "Namespaces and resolution", and `docs/spec/01-source-language.md`,
//! "Declarations". Resolution has already reported every name fault, so the
//! names here are looked up without reporting: a name that does not resolve
//! leaves an error type that reports nothing again.
//!
//! The checker supports `fn` declarations with bare-name parameters and
//! written scalar, variant, and `Fn` types; module values of those types;
//! variant types whose cases have no fields, with their members; and, in the
//! embedded standard library, the intrinsic scalar types with their external
//! members, the `Str` and `Bytes` wrappers, and interfaces whose members are
//! abstract and not generic, with the implementations of them. Any other
//! declaration of a local module is `@tool.unavailable` over the declaration;
//! one of the standard library is skipped with a note.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::compiler::CompilerIntrinsic;
use vibra_ir::{FnType, Library, Type, TypeId};
use vibra_syntax::source::{
    Callable, Declaration, ExprForm, Impl, InterfaceDecl, Module, ModuleLet,
    PatternForm, Primary, SourceOptions, Suffix, TypeBody, TypeDecl, TypeExpr,
    TypeForm,
};
use vibra_syntax::{Document, parse_source, parse_source_with};

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
    /// A declaration this checker has no table for: a `proc` or a `test`.
    Other(usize, ByteSpan),
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
    /// Whether its declarations are checked and lowered.
    pub(crate) in_scope: bool,
    /// The typed view.
    pub(crate) view: Option<Module<'a>>,
    /// The flat module scope: the top-level declarations and the imports.
    pub(crate) scope: BTreeMap<String, Entity>,
}

/// The shape of a declared type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeKind {
    /// An intrinsic scalar type of the standard library.
    Scalar(Type),
    /// A variant type whose cases have no fields, in written order.
    Variant(Vec<String>),
    /// A wrapper type of the standard library over its representation.
    Wrapper(Type),
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
    /// The checked IR identity of a variant or wrapper type.
    pub(crate) id: Option<TypeId>,
    /// The members, by name.
    pub(crate) members: BTreeMap<String, usize>,
}

impl TypeData<'_> {
    /// The type the declaration denotes, when this checker supports it.
    pub(crate) fn self_type(&self) -> Option<Type> {
        match (&self.kind, &self.id) {
            (TypeKind::Scalar(ty), _) => Some(ty.clone()),
            (TypeKind::Variant(_) | TypeKind::Wrapper(_), Some(id)) => {
                Some(Type::Declared(id.clone(), Vec::new()))
            }
            _ => None,
        }
    }
}

/// One `fn`: top level, a member of a type, or a member of an `impl`.
#[derive(Debug)]
pub(crate) struct FnData<'a> {
    pub(crate) module: usize,
    /// The type it is a member of.
    pub(crate) owner: Option<usize>,
    /// The interface it implements, for a member of an `impl`.
    pub(crate) implements: Option<usize>,
    pub(crate) name: String,
    pub(crate) view: Callable<'a>,
    pub(crate) identity: String,
    /// The qualified name the checked IR records.
    pub(crate) qualified: String,
    /// The signature, when this checker supports the header.
    pub(crate) signature: Option<FnType>,
    /// Each parameter's name and span, in order.
    pub(crate) params: Vec<(String, ByteSpan)>,
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

/// One abstract member of a supported interface.
#[derive(Clone, Debug)]
pub(crate) struct InterfaceMemberData {
    pub(crate) name: String,
    /// Its type, `Self` written as [`Type::SelfType`].
    pub(crate) signature: FnType,
    /// The receiver slot: the first positional parameter written `Self`.
    pub(crate) receiver: Option<usize>,
}

/// One `interface`.
#[derive(Debug)]
pub(crate) struct InterfaceData<'a> {
    pub(crate) module: usize,
    pub(crate) name: String,
    pub(crate) view: InterfaceDecl<'a>,
    /// The checked IR identity, when this checker supports the interface.
    pub(crate) id: Option<TypeId>,
    pub(crate) members: Vec<InterfaceMemberData>,
}

/// One supported implementation: an `impl` of a supported interface in a
/// supported type.
#[derive(Debug)]
pub(crate) struct ImplData {
    pub(crate) owner: usize,
    pub(crate) interface: usize,
    pub(crate) receiver: Type,
    /// The function supplying each member.
    pub(crate) members: Vec<(String, usize)>,
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
    local_package: PackageName,
    embedded_package: PackageName,
}

/// Why a written type has no checked type here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoType {
    /// The form is outside this checker's support.
    Unsupported,
    /// A name that resolution already reported.
    Unresolved,
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
            interfaces: Vec::new(),
            impls: Vec::new(),
            prelude: BTreeMap::new(),
            str_type: None,
            bool_type: None,
            notes: Vec::new(),
            diagnostics: Vec::new(),
            library: Vec::new(),
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
        program.type_kinds();
        program.interface_headers();
        program.function_headers();
        program.value_headers();
        program.implementations(pending_impls);
        program.str_type = inputs
            .str_role
            .as_ref()
            .and_then(|(module, name)| program.role_type(module, name));
        program.library = inputs
            .library
            .iter()
            .filter_map(|(library, module, name)| {
                program
                    .top_level(module, name)
                    .map(|entity| (*library, entity))
            })
            .collect();
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
        pending_impls: &mut Vec<(usize, Impl<'a>)>,
    ) {
        let module_index = self.modules.len();
        let view = vibra_syntax::module(document);
        self.modules.push(ModuleData {
            path: source.path.clone(),
            source_id: Arc::from(source.source_id.as_str()),
            embedded: source.embedded,
            in_scope: false,
            view,
            scope: BTreeMap::new(),
        });
        let Some(view) = view else { return };
        let package = if source.embedded {
            &inputs.embedded
        } else {
            &inputs.local
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
                    self.types.push(TypeData {
                        module: module_index,
                        name: type_name.clone(),
                        view: decl,
                        identity: identity("", "@type", name.text()),
                        kind: TypeKind::Unsupported,
                        id: None,
                        members: BTreeMap::new(),
                    });
                    self.declare(module_index, &type_name, Entity::Type(index));
                    for member in decl.members() {
                        let Some(member_name) = member.name() else {
                            continue;
                        };
                        let fn_index = self.functions.len();
                        self.functions.push(FnData {
                            module: module_index,
                            owner: Some(index),
                            implements: None,
                            name: member_name.text().to_owned(),
                            view: member,
                            identity: identity(&type_name, "@fn", member_name.text()),
                            qualified: format!(
                                "{dotted}.{type_name}.{}",
                                member_name.text()
                            ),
                            signature: None,
                            params: Vec::new(),
                            external: None,
                        });
                        if let Some(data) = self.types.get_mut(index) {
                            data.members
                                .insert(member_name.text().to_owned(), fn_index);
                        }
                    }
                    for block in decl.impls() {
                        pending_impls.push((index, block));
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
                        members: Vec::new(),
                    });
                    self.declare(module_index, name.text(), Entity::Interface(index));
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
                        signature: None,
                        params: Vec::new(),
                        external: None,
                    });
                    self.declare(module_index, name.text(), Entity::Fn(index));
                }
                Declaration::Proc(callable) => {
                    if let Some(name) = callable.name() {
                        let span = callable.construct().span();
                        self.declare(
                            module_index,
                            name.text(),
                            Entity::Other(module_index, span),
                        );
                    }
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
                Declaration::Test(_) => {}
            }
        }
    }

    fn declare(&mut self, module: usize, name: &str, entity: Entity) {
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

    /// Marks the embedded modules, the modules of `units`, and every module
    /// they import, transitively, as checked.
    fn mark_scope(&mut self, units: &[String]) {
        let mut pending = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            let unit = module.path.split('.').next().unwrap_or(&module.path);
            if module.embedded || units.iter().any(|selected| selected == unit) {
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
                    Entity::Other(module, _) => Some(module),
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

    /// The checked type of a written type expression, read in `module`, with
    /// `self_type` standing for `Self`.
    pub(crate) fn written(
        &self,
        module: usize,
        written: TypeExpr<'_>,
        self_type: Option<&Type>,
    ) -> Result<Type, NoType> {
        self.written_at(module, written, self_type, 0)
    }

    fn written_at(
        &self,
        module: usize,
        written: TypeExpr<'_>,
        self_type: Option<&Type>,
        depth: usize,
    ) -> Result<Type, NoType> {
        if depth >= Type::MAX_DEPTH {
            return Err(NoType::Unsupported);
        }
        match written.form().ok_or(NoType::Unresolved)? {
            TypeForm::Path {
                path,
                positional,
                labelled,
            } => {
                if !positional.is_empty() || !labelled.is_empty() {
                    return Err(NoType::Unsupported);
                }
                let segments = path.segments();
                let (first, rest) = segments.split_first().ok_or(NoType::Unresolved)?;
                if first.text() == "Self" && rest.is_empty() {
                    return self_type.cloned().ok_or(NoType::Unsupported);
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
                match entity {
                    Entity::Type(index) => self
                        .types
                        .get(index)
                        .and_then(TypeData::self_type)
                        .ok_or(NoType::Unsupported),
                    Entity::Interface(_) => Err(NoType::Unsupported),
                    _ => Err(NoType::Unresolved),
                }
            }
            TypeForm::Atom(atom) => Ok(Type::AtomSingleton(
                atom.text()
                    .strip_prefix('@')
                    .unwrap_or(atom.text())
                    .to_owned(),
            )),
            TypeForm::Fn { inputs, result } => {
                if !inputs.labelled().is_empty() {
                    return Err(NoType::Unsupported);
                }
                let parameters = inputs
                    .positional()
                    .into_iter()
                    .map(|input| self.written_at(module, input, self_type, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                let result = self.written_at(module, result, self_type, depth + 1)?;
                Ok(Type::function(FnType::new(parameters, result)))
            }
            TypeForm::Tuple(_)
            | TypeForm::Record(_)
            | TypeForm::Proc { .. }
            | TypeForm::Intent { .. } => Err(NoType::Unsupported),
        }
    }

    /// Decides the shape of every type.
    fn type_kinds(&mut self) {
        let mut kinds = Vec::new();
        for (index, data) in self.types.iter().enumerate() {
            let Some(module) = self.modules.get(data.module) else {
                continue;
            };
            let plain = data.view.params().is_empty()
                && data.view.given().is_none()
                && data.view.invariant().is_none();
            let kind = match data.view.body() {
                Some(TypeBody::Intrinsic(tag)) if plain => {
                    scalar(tag.text()).map_or(TypeKind::Unsupported, TypeKind::Scalar)
                }
                Some(TypeBody::Cases(cases))
                    if plain && cases.iter().all(|case| case.fields().is_none()) =>
                {
                    let names = cases
                        .iter()
                        .filter_map(|case| {
                            case.name().map(|name| name.text().to_owned())
                        })
                        .collect::<Vec<_>>();
                    if names.len() == cases.len() {
                        TypeKind::Variant(names)
                    } else {
                        TypeKind::Unsupported
                    }
                }
                Some(TypeBody::Of(written)) if plain && module.embedded => {
                    list_of_scalar(written)
                        .map_or(TypeKind::Unsupported, TypeKind::Wrapper)
                }
                _ => TypeKind::Unsupported,
            };
            kinds.push((index, kind));
        }
        for (index, kind) in kinds {
            let module = self.types.get(index).map(|data| data.module);
            let span = self
                .types
                .get(index)
                .map(|data| data.view.construct().span());
            let unsupported = kind == TypeKind::Unsupported;
            if let Some(data) = self.types.get_mut(index) {
                if matches!(kind, TypeKind::Variant(_) | TypeKind::Wrapper(_)) {
                    let path = format!(
                        "{}.{}",
                        self.modules
                            .get(data.module)
                            .map(|module| module.path.trim_start_matches('@'))
                            .unwrap_or_default(),
                        data.name
                    );
                    data.id = TypeId::new(path, data.identity.clone()).ok();
                }
                data.kind = kind;
            }
            if unsupported && let (Some(module), Some(span)) = (module, span) {
                let what = format!(
                    "the type `{}`",
                    self.types
                        .get(index)
                        .map(|data| data.name.as_str())
                        .unwrap_or_default()
                );
                self.unsupported(module, span, &what);
            }
        }
    }

    /// Reads every interface whose members are abstract, non-generic `fn`
    /// members of written supported types.
    fn interface_headers(&mut self) {
        let mut results = Vec::new();
        for (index, data) in self.interfaces.iter().enumerate() {
            let view = data.view;
            let mut members = Vec::new();
            let mut supported = view.params().is_empty()
                && view.given().is_none()
                && view.assoc_types().is_empty()
                && self
                    .modules
                    .get(data.module)
                    .is_some_and(|module| module.embedded);
            for member in view.members() {
                if !supported {
                    break;
                }
                let Some(name) = member.name() else {
                    supported = false;
                    break;
                };
                if member.body().is_some() || !plain_callable(&member) {
                    supported = false;
                    break;
                }
                match self.callable_type(data.module, &member, Some(&Type::SelfType)) {
                    Some((signature, _)) => {
                        let receiver = signature
                            .parameters
                            .iter()
                            .position(|parameter| *parameter == Type::SelfType);
                        members.push(InterfaceMemberData {
                            name: name.text().to_owned(),
                            signature,
                            receiver,
                        });
                    }
                    None => supported = false,
                }
            }
            results.push((index, supported.then_some(members)));
        }
        for (index, members) in results {
            let Some(data) = self.interfaces.get(index) else {
                continue;
            };
            let module = data.module;
            let span = data.view.construct().span();
            let name = data.name.clone();
            match members {
                Some(members) => {
                    let path = self
                        .modules
                        .get(module)
                        .map(|module| module.path.trim_start_matches('@').to_owned());
                    let id = path.and_then(|path| {
                        self.interface_identity(index).and_then(|seed| {
                            TypeId::new(format!("{path}.{name}"), seed).ok()
                        })
                    });
                    if let Some(data) = self.interfaces.get_mut(index) {
                        data.id = id;
                        data.members = members;
                    }
                }
                None => {
                    self.unsupported(module, span, &format!("the interface `{name}`"))
                }
            }
        }
    }

    /// The identity of an interface: its anchor in its package.
    fn interface_identity(&self, index: usize) -> Option<String> {
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

    fn identity_of(
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
            &self.local_package
        };
        declaration_identity(&package.name, &package.version, module, owner, kind, name)
    }

    /// The type of a callable header with bare-name parameters and written
    /// supported types, with the parameter names; `None` otherwise.
    pub(crate) fn callable_type(
        &self,
        module: usize,
        view: &Callable<'_>,
        self_type: Option<&Type>,
    ) -> Option<(FnType, Vec<(String, ByteSpan)>)> {
        let mut parameters = Vec::new();
        let mut names = Vec::new();
        for (pattern, written) in view.takes() {
            let PatternForm::Path(path) = pattern.form() else {
                return None;
            };
            let segments = path.segments();
            let [name] = segments.as_slice() else {
                return None;
            };
            names.push((name.text().to_owned(), name.span()));
            parameters.push(self.written(module, written, self_type).ok()?);
        }
        let (_, result) = view.result()?;
        let result = self.written(module, result, self_type).ok()?;
        Some((FnType::new(parameters, result), names))
    }

    /// Reads the header of every `fn`.
    fn function_headers(&mut self) {
        let mut results = Vec::new();
        for (index, data) in self.functions.iter().enumerate() {
            let owner = data.owner.and_then(|owner| self.types.get(owner));
            let owner_supported = data.owner.is_none()
                || owner.is_some_and(|owner| owner.kind != TypeKind::Unsupported);
            if !owner_supported {
                results.push((index, None, None));
                continue;
            }
            let self_type = owner.and_then(TypeData::self_type);
            let external = data
                .view
                .symbol()
                .and_then(|symbol| symbol.value())
                .and_then(|symbol| CompilerIntrinsic::from_symbol(&symbol));
            let header = plain_callable(&data.view)
                .then(|| {
                    self.callable_type(data.module, &data.view, self_type.as_ref())
                })
                .flatten()
                .filter(|_| external.is_some() || data.view.body().is_some());
            results.push((index, header, external));
        }
        for (index, header, external) in results {
            let Some(data) = self.functions.get(index) else {
                continue;
            };
            let report = data.owner.is_none_or(|owner| {
                self.types
                    .get(owner)
                    .is_some_and(|owner| owner.kind != TypeKind::Unsupported)
            });
            let module = data.module;
            let span = data.view.construct().span();
            let what = format!("the function `{}`", data.qualified);
            match header {
                Some((signature, params)) => {
                    if let Some(data) = self.functions.get_mut(index) {
                        data.signature = Some(signature);
                        data.params = params;
                        data.external = external;
                    }
                }
                None if report => self.unsupported(module, span, &what),
                None => {}
            }
        }
        for module in 0..self.modules.len() {
            let Some(view) = self.modules.get(module).and_then(|data| data.view) else {
                continue;
            };
            for declaration in view.declarations() {
                match declaration {
                    Declaration::Proc(callable) => {
                        let what = format!(
                            "the procedure `{}`",
                            callable.name().map(|name| name.text()).unwrap_or_default()
                        );
                        self.unsupported(module, callable.construct().span(), &what);
                    }
                    Declaration::Test(test) => {
                        self.unsupported(module, test.construct().span(), "the test");
                    }
                    Declaration::Interface(_)
                    | Declaration::Use(_)
                    | Declaration::Type(_)
                    | Declaration::Fn(_)
                    | Declaration::Let(_) => {}
                }
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
                .and_then(|written| self.written(data.module, written, None).ok());
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
                Some(ty) => {
                    if let Some(data) = self.values.get_mut(index) {
                        data.ty = Some(ty);
                    }
                }
                None => self.unsupported(module, span, &what),
            }
        }
    }

    /// Records every `impl` of a supported interface whose members all have
    /// supported headers of the implemented member's type.
    fn implementations(&mut self, pending: Vec<(usize, Impl<'a>)>) {
        for (owner, block) in pending {
            let Some(data) = self.types.get(owner) else {
                continue;
            };
            let module = data.module;
            let receiver = data.self_type();
            let interface = block.target().and_then(|target| match target.form() {
                Some(TypeForm::Path {
                    path,
                    positional,
                    labelled,
                }) if positional.is_empty() && labelled.is_empty() => {
                    match path.segments().as_slice() {
                        [name] => match self.lookup(module, name.text()) {
                            Some(Entity::Interface(index)) => Some(index),
                            _ => None,
                        },
                        _ => None,
                    }
                }
                _ => None,
            });
            let span = block.node().span();
            let target_name = block
                .target()
                .and_then(|target| match target.form() {
                    Some(TypeForm::Path { path, .. }) => Some(path.dotted()),
                    _ => None,
                })
                .unwrap_or_default();
            let what =
                format!("the implementation of `{target_name}` for `{}`", data.name);
            let supported = interface
                .and_then(|index| self.interfaces.get(index).map(|data| (index, data)))
                .filter(|(_, data)| data.id.is_some());
            let (Some(receiver), Some((interface, interface_data))) =
                (receiver, supported)
            else {
                self.unsupported(module, span, &what);
                continue;
            };
            let expected = interface_data.members.clone();
            let interface_name = interface_data.name.clone();
            let mut members = Vec::new();
            let mut complete = block.choices().is_empty();
            for member in block.members() {
                let Some(name) = member.name() else {
                    complete = false;
                    continue;
                };
                let header = plain_callable(&member)
                    .then(|| self.callable_type(module, &member, Some(&receiver)))
                    .flatten();
                let declared = expected
                    .iter()
                    .find(|declared| declared.name == name.text());
                let fits = match (&header, declared) {
                    (Some((signature, _)), Some(declared)) => {
                        *signature == substitute_self(&declared.signature, &receiver)
                    }
                    _ => false,
                };
                let external = member
                    .symbol()
                    .and_then(|symbol| symbol.value())
                    .and_then(|symbol| CompilerIntrinsic::from_symbol(&symbol));
                if !fits || (external.is_none() && member.body().is_none()) {
                    complete = false;
                    continue;
                }
                let Some((signature, params)) = header else {
                    continue;
                };
                let owner_name = self
                    .types
                    .get(owner)
                    .map(|data| data.name.clone())
                    .unwrap_or_default();
                let dotted = self
                    .modules
                    .get(module)
                    .map(|data| data.path.trim_start_matches('@').to_owned())
                    .unwrap_or_default();
                let fn_index = self.functions.len();
                self.functions.push(FnData {
                    module,
                    owner: Some(owner),
                    implements: Some(interface),
                    name: name.text().to_owned(),
                    view: member,
                    identity: String::new(),
                    qualified: format!(
                        "{dotted}.{owner_name}.{interface_name}.{}",
                        name.text()
                    ),
                    signature: Some(signature),
                    params,
                    external,
                });
                members.push((name.text().to_owned(), fn_index));
            }
            let all = expected
                .iter()
                .all(|declared| members.iter().any(|(name, _)| *name == declared.name));
            if complete && all && members.len() == expected.len() {
                self.impls.push(ImplData {
                    owner,
                    interface,
                    receiver,
                    members,
                });
            } else {
                for (_, index) in &members {
                    if let Some(data) = self.functions.get_mut(*index) {
                        data.signature = None;
                    }
                }
                self.unsupported(module, span, &what);
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
                    return match &data.kind {
                        TypeKind::Variant(cases)
                            if cases.iter().any(|known| known == case) =>
                        {
                            Some((ty, (*case).to_owned()))
                        }
                        _ => None,
                    };
                }
                (Entity::Value(next), [], false) => current = next,
                _ => return None,
            }
        }
        None
    }

    /// The implementation of `interface` for `receiver`.
    pub(crate) fn implementation(
        &self,
        interface: usize,
        receiver: &Type,
    ) -> Option<&ImplData> {
        self.impls.iter().find(|data| {
            data.interface == interface
                && data.receiver == *receiver
                && self
                    .types
                    .get(data.owner)
                    .and_then(|owner| self.modules.get(owner.module))
                    .is_some_and(|module| module.in_scope)
        })
    }
}

/// Whether a callable header has only what this checker supports besides
/// its types: no type parameters, labelled parameters, guards, or row.
fn plain_callable(view: &Callable<'_>) -> bool {
    view.params().is_empty()
        && view.labelled().is_empty()
        && view.given().is_none()
        && view.requires().is_none()
        && view.ensures().is_none()
        && view.performs().is_none()
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

/// The representation `List(Char)` or `List(U8)` of a standard-library
/// wrapper.
fn list_of_scalar(written: TypeExpr<'_>) -> Option<Type> {
    let TypeForm::Path {
        path,
        positional,
        labelled,
    } = written.form()?
    else {
        return None;
    };
    if path.dotted() != "List" || !labelled.is_empty() {
        return None;
    }
    let [element] = positional.as_slice() else {
        return None;
    };
    let TypeForm::Path {
        path,
        positional,
        labelled,
    } = element.form()?
    else {
        return None;
    };
    if !positional.is_empty() || !labelled.is_empty() {
        return None;
    }
    let element = match path.dotted().as_str() {
        "Char" => Type::Char,
        "U8" => Type::U8,
        _ => return None,
    };
    Some(Type::list(element))
}

/// `signature` with `Self` replaced by `receiver`.
pub(crate) fn substitute_self(signature: &FnType, receiver: &Type) -> FnType {
    let replace = |ty: &Type| {
        if *ty == Type::SelfType {
            receiver.clone()
        } else {
            ty.clone()
        }
    };
    FnType::new(
        signature.parameters.iter().map(replace).collect(),
        replace(&signature.result),
    )
}
