//! What the names of the standard library denote, as far as the load checks
//! need: the declarations that play a role, the three core types, and the
//! builtin types; and the reading of a written type expression into the type
//! language of the registry rows.
//!
//! This is not resolution. It reads the names a standard-library module can
//! write in a signature: a type parameter, `Self`, an associated type or the
//! choice of one, a declaration of the module, an import, and a prelude name.

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir::external::RowType;
use vibra_ir::{Library, Type};
use vibra_syntax::TypeExpr;
use vibra_syntax::source::{Declaration, Module, TypeBody, TypeForm};

use super::Role;

/// The three core types and the module that declares them
/// (`docs/spec/04-programs-and-packages.md`, "The standard library").
pub(crate) const CORE_TYPES: [(Library, &str); 3] = [
    (Library::Ordering, "Ordering"),
    (Library::ArithmeticError, "ArithmeticError"),
    (Library::ConversionError, "ConversionError"),
];

/// The module that declares the three core types.
pub(crate) const CORE_MODULE: &str = "@std.core";

/// The module that declares the builtin types.
pub(crate) const BUILTIN_MODULE: &str = "@std.builtin";

/// What a declaration of the standard library is, for the load checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// A declaration the IR names by what it does.
    Library(Library),
    /// A builtin scalar type, by its wire tag.
    Core(Type),
    /// The builtin `List(T)`.
    List,
}

/// The compiler-owned type of the wire tag `tag` (with its `@`), or `None`
/// for a tag that names none.
pub(crate) fn intrinsic(tag: &str) -> Option<Named> {
    Some(match tag {
        "@void" => Named::Core(Type::Void),
        "@never" => Named::Core(Type::Never),
        "@atom" => Named::Core(Type::Atom),
        "@char" => Named::Core(Type::Char),
        "@i8" => Named::Core(Type::I8),
        "@i16" => Named::Core(Type::I16),
        "@i32" => Named::Core(Type::I32),
        "@i64" => Named::Core(Type::I64),
        "@u8" => Named::Core(Type::U8),
        "@u16" => Named::Core(Type::U16),
        "@u32" => Named::Core(Type::U32),
        "@u64" => Named::Core(Type::U64),
        "@f32" => Named::Core(Type::F32),
        "@f64" => Named::Core(Type::F64),
        "@list" => Named::List,
        _ => return None,
    })
}

/// The names of the standard library's declarations that the load checks
/// read: every declaration that plays a role, the core types, and the
/// builtin types, keyed by module path atom and name; and the prelude.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryNames {
    modules: BTreeSet<String>,
    declarations: BTreeMap<(String, String), Named>,
    prelude: BTreeMap<String, (String, String)>,
}

impl LibraryNames {
    /// The names of the parsed `modules`, each with its module path atom,
    /// and the prelude bound to (module, name) pairs.
    pub(crate) fn new(
        modules: &[(&str, Module<'_>)],
        prelude: BTreeMap<String, (String, String)>,
    ) -> Self {
        let mut names = Self {
            modules: modules.iter().map(|(path, _)| (*path).to_owned()).collect(),
            declarations: BTreeMap::new(),
            prelude,
        };
        for (path, module) in modules {
            for declaration in module.declarations() {
                let (name, named) = match &declaration {
                    Declaration::Type(decl) => {
                        let Some(name) = decl.name() else { continue };
                        let role = decl
                            .role()
                            .and_then(|role| role.tag())
                            .and_then(|tag| Role::from_atom(tag.text()));
                        let named = match (decl.body(), role) {
                            (_, Some(role)) => Some(Named::Library(role.library())),
                            (Some(TypeBody::Intrinsic(tag)), None) => {
                                intrinsic(tag.text())
                            }
                            _ if *path == CORE_MODULE => CORE_TYPES
                                .iter()
                                .find(|(_, core)| *core == name.text())
                                .map(|(library, _)| Named::Library(*library)),
                            _ => None,
                        };
                        (name.text(), named)
                    }
                    Declaration::Interface(decl) => {
                        let Some(name) = decl.name() else { continue };
                        let named = decl
                            .role()
                            .and_then(|role| role.tag())
                            .and_then(|tag| Role::from_atom(tag.text()))
                            .map(|role| Named::Library(role.library()));
                        (name.text(), named)
                    }
                    _ => continue,
                };
                if let Some(named) = named {
                    names
                        .declarations
                        .insert(((*path).to_owned(), name.to_owned()), named);
                }
            }
        }
        names
    }

    fn named(&self, module: &str, name: &str) -> Option<&Named> {
        self.declarations.get(&(module.to_owned(), name.to_owned()))
    }

    /// What the declaration `name` of the module `module` is.
    pub(crate) fn declaration(&self, module: &str, name: &str) -> Option<&Named> {
        self.named(module, name)
    }
}

/// A type in the language of the registry rows, extended with the open
/// forms of an interface: `Self` and an associated type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Written {
    /// A compiler-owned scalar type.
    Core(Type),
    /// The type parameter at this position of the declaration's binders.
    Binder(usize),
    /// `List(T)`.
    List(Box<Self>),
    /// A structural tuple.
    Tuple(Vec<Self>),
    /// `Fn({…} R)` without labels.
    Fn(Vec<Self>, Box<Self>),
    /// A library declaration applied to its arguments.
    Library(Library, Vec<Self>),
    /// `Self` inside an interface.
    SelfType,
    /// An associated type, by name, inside its interface.
    Associated(String),
}

impl Written {
    /// The row type `row`, its generics read as binders.
    pub(crate) fn of_row(row: &RowType) -> Self {
        let all = |rows: &[RowType]| rows.iter().map(Self::of_row).collect();
        match row {
            RowType::Core(ty) => Self::Core(ty.clone()),
            RowType::Generic(index) => Self::Binder(*index),
            RowType::List(element) => Self::List(Box::new(Self::of_row(element))),
            RowType::Tuple(components) => Self::Tuple(all(components)),
            RowType::Fn(parameters, result) => {
                Self::Fn(all(parameters), Box::new(Self::of_row(result)))
            }
            RowType::Library(library, arguments) => {
                Self::Library(*library, all(arguments))
            }
        }
    }
}

/// What an import of a module brings into scope.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Import {
    /// A module, by path atom.
    Module(String),
    /// A top-level declaration: its module path atom and its name.
    Declaration(String, String),
}

/// The names a signature of one module can write, at one declaration.
#[derive(Clone, Debug)]
pub(crate) struct Scope<'s> {
    module: &'s str,
    names: &'s LibraryNames,
    imports: BTreeMap<String, Import>,
    binders: Vec<String>,
    self_type: Option<Written>,
    associated: Vec<String>,
    choices: Vec<(String, Written)>,
}

impl<'s> Scope<'s> {
    /// The scope at the top level of `module`, whose path atom is `path`.
    pub(crate) fn module(
        path: &'s str,
        module: &Module<'_>,
        names: &'s LibraryNames,
    ) -> Self {
        let mut imports = BTreeMap::new();
        for import in module.uses() {
            let (Some(locator), Some(local)) = (import.locator(), import.name()) else {
                continue;
            };
            let target = locator.text();
            let import = if names.modules.contains(target) {
                Import::Module(target.to_owned())
            } else {
                match target.rsplit_once('.') {
                    Some((module, name)) => {
                        Import::Declaration(module.to_owned(), name.to_owned())
                    }
                    None => continue,
                }
            };
            imports.insert(local.to_owned(), import);
        }
        Self {
            module: path,
            names,
            imports,
            binders: Vec::new(),
            self_type: None,
            associated: Vec::new(),
            choices: Vec::new(),
        }
    }

    /// This scope with `binders` appended to its type parameters.
    pub(crate) fn with_binders<'b>(
        mut self,
        binders: impl IntoIterator<Item = &'b str>,
    ) -> Self {
        self.binders.extend(binders.into_iter().map(str::to_owned));
        self
    }

    /// This scope with `Self` denoting `self_type`.
    pub(crate) fn with_self(mut self, self_type: Option<Written>) -> Self {
        self.self_type = self_type;
        self
    }

    /// This scope inside an interface with the associated types `names`.
    pub(crate) fn with_associated<'b>(
        mut self,
        names: impl IntoIterator<Item = &'b str>,
    ) -> Self {
        self.associated.extend(names.into_iter().map(str::to_owned));
        self
    }

    /// This scope inside an `impl` whose associated type `name` is `chosen`.
    pub(crate) fn with_choice(mut self, name: &str, chosen: Written) -> Self {
        self.choices.push((name.to_owned(), chosen));
        self
    }

    /// The number of type parameters in scope.
    pub(crate) fn binder_count(&self) -> usize {
        self.binders.len()
    }

    /// The declared type `name` of this module applied to its own parameters,
    /// which are the first binders of the scope: the meaning of `Self` in its
    /// body.
    pub(crate) fn declared_self(
        &self,
        name: &str,
        parameters: usize,
    ) -> Option<Written> {
        let arguments = (0..parameters).map(Written::Binder).collect();
        apply(self.names.declaration(self.module, name)?, arguments)
    }

    /// What the declaration named `name` denotes from this module.
    fn lookup(&self, segments: &[&str]) -> Option<&'s Named> {
        match segments {
            [name] => {
                if let Some(named) = self.names.declaration(self.module, name) {
                    return Some(named);
                }
                match self.imports.get(*name) {
                    Some(Import::Declaration(module, name)) => {
                        return self.names.declaration(module, name);
                    }
                    Some(Import::Module(_)) => return None,
                    None => {}
                }
                let (module, name) = self.names.prelude.get(*name)?;
                self.names.declaration(module, name)
            }
            [module, name] => match self.imports.get(*module)? {
                Import::Module(module) => self.names.declaration(module, name),
                Import::Declaration(..) => None,
            },
            _ => None,
        }
    }

    /// The written type expression `ty` in the type language of the rows, or
    /// `None` when it names something no row can hold: an unknown name, a
    /// label, an atom, a record, a `Proc` or `Intent`, or a type nested
    /// deeper than the language allows.
    pub(crate) fn read(&self, ty: &TypeExpr<'_>) -> Option<Written> {
        self.read_at(ty, 0)
    }

    fn read_at(&self, ty: &TypeExpr<'_>, depth: usize) -> Option<Written> {
        if depth >= Type::MAX_DEPTH {
            return None;
        }
        let all = |types: &[TypeExpr<'_>]| {
            types
                .iter()
                .map(|ty| self.read_at(ty, depth + 1))
                .collect::<Option<Vec<_>>>()
        };
        match ty.form()? {
            TypeForm::Path {
                path,
                positional,
                labelled,
            } => {
                if !labelled.is_empty() {
                    return None;
                }
                let segments = path.segments();
                let segments = segments
                    .iter()
                    .map(|segment| segment.text())
                    .collect::<Vec<_>>();
                let arguments = all(&positional)?;
                if let [name] = segments.as_slice() {
                    if let Some(index) =
                        self.binders.iter().position(|binder| binder == name)
                    {
                        return arguments.is_empty().then_some(Written::Binder(index));
                    }
                    if *name == "Self" {
                        return self.self_type.clone().filter(|_| arguments.is_empty());
                    }
                    if self.associated.iter().any(|associated| associated == name) {
                        return arguments
                            .is_empty()
                            .then(|| Written::Associated((*name).to_owned()));
                    }
                    if let Some((_, chosen)) =
                        self.choices.iter().find(|(choice, _)| choice == name)
                    {
                        return arguments.is_empty().then(|| chosen.clone());
                    }
                }
                apply(self.lookup(&segments)?, arguments)
            }
            TypeForm::Tuple(components) => Some(Written::Tuple(all(&components)?)),
            TypeForm::Fn { inputs, result } => {
                if !inputs.labelled().is_empty() {
                    return None;
                }
                Some(Written::Fn(
                    all(&inputs.positional())?,
                    Box::new(self.read_at(&result, depth + 1)?),
                ))
            }
            TypeForm::Atom(_)
            | TypeForm::Record(_)
            | TypeForm::Proc { .. }
            | TypeForm::Intent { .. } => None,
        }
    }
}

/// `named` applied to `arguments`, when the arity fits.
fn apply(named: &Named, arguments: Vec<Written>) -> Option<Written> {
    match named {
        Named::Library(library) => (arguments.len() == library.arity())
            .then_some(Written::Library(*library, arguments)),
        Named::Core(ty) => arguments.is_empty().then(|| Written::Core(ty.clone())),
        Named::List => {
            let mut arguments = arguments;
            match (arguments.pop(), arguments.is_empty()) {
                (Some(element), true) => Some(Written::List(Box::new(element))),
                _ => None,
            }
        }
    }
}
