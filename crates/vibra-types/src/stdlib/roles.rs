//! The language roles (`docs/spec/02-type-system.md`, "Language core and
//! standard library"): each is claimed exactly once, with the `role` section,
//! on a declaration of the shape the role requires.

use std::collections::BTreeMap;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{Library, Type};
use vibra_syntax::source::{
    Callable, Declaration, InterfaceDecl, Module, TypeBody, TypeDecl,
};

use super::Fault;
use super::names::{CORE_MODULE, CORE_TYPES, LibraryNames, Scope, Written};

/// A language role: a closed, compiler-known position that one declaration
/// of the standard library plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// `@bool`: `if` conditions, guard predicates, `true` and `false`.
    Bool,
    /// `@str`: string literals.
    Str,
    /// `@bytes`: byte lookups.
    Bytes,
    /// `@option`: lookups, `try` on an optional value, optional grants.
    Option,
    /// `@result`: `try`, unhandled-failure checking, fallible construction.
    Result,
    /// `@dict`: dict literals.
    Dict,
    /// `@iter`: iteration contracts.
    Iter,
    /// `@equatable`: structural equality of key types.
    Equatable,
    /// `@ordered`: dict keys.
    Ordered,
}

/// The role atoms, in the order of the type chapter's table.
pub(crate) const ROLE_ATOMS: [&str; 9] = [
    "@bool",
    "@str",
    "@bytes",
    "@option",
    "@result",
    "@dict",
    "@iter",
    "@equatable",
    "@ordered",
];

impl Role {
    /// Every role, in the order of the type chapter's table.
    pub const ALL: [Self; 9] = [
        Self::Bool,
        Self::Str,
        Self::Bytes,
        Self::Option,
        Self::Result,
        Self::Dict,
        Self::Iter,
        Self::Equatable,
        Self::Ordered,
    ];

    /// The role atom, such as `@bool`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Bool => "@bool",
            Self::Str => "@str",
            Self::Bytes => "@bytes",
            Self::Option => "@option",
            Self::Result => "@result",
            Self::Dict => "@dict",
            Self::Iter => "@iter",
            Self::Equatable => "@equatable",
            Self::Ordered => "@ordered",
        }
    }

    /// The role whose atom is `atom`.
    #[must_use]
    pub fn from_atom(atom: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.atom() == atom)
    }

    /// The library declaration the checked IR names for this role.
    #[must_use]
    pub const fn library(self) -> Library {
        match self {
            Self::Bool => Library::Bool,
            Self::Str => Library::Str,
            Self::Bytes => Library::Bytes,
            Self::Option => Library::Option,
            Self::Result => Library::Result,
            Self::Dict => Library::Dict,
            Self::Iter => Library::Iter,
            Self::Equatable => Library::Equatable,
            Self::Ordered => Library::Ordered,
        }
    }
}

/// A declaration that claims a role.
#[derive(Clone, Debug)]
pub(crate) struct Claim {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
}

/// One parsed module of the standard library.
pub(crate) struct Parsed<'a> {
    pub(crate) path: &'a str,
    pub(crate) source_id: &'a str,
    pub(crate) module: Module<'a>,
}

/// The declaration that claims each role, and the faults of the claims: a
/// role claimed by no declaration, by several, or by a declaration of the
/// wrong shape.
pub(crate) fn check_roles(
    modules: &[Parsed<'_>],
    names: &LibraryNames,
) -> (BTreeMap<Role, Claim>, Vec<Fault>) {
    let mut claims: BTreeMap<Role, Vec<Claim>> = BTreeMap::new();
    let mut faults = Vec::new();
    for parsed in modules {
        for declaration in parsed.module.declarations() {
            let top = Scope::module(parsed.path, &parsed.module, names);
            let (role, name, kind, problem) = match &declaration {
                Declaration::Type(decl) => {
                    let Some(role) = decl.role().and_then(|role| role.tag()) else {
                        continue;
                    };
                    let problem = Role::from_atom(role.text())
                        .and_then(|role| type_shape(role, decl, top));
                    (role, decl.name(), "@type", problem)
                }
                Declaration::Interface(decl) => {
                    let Some(role) = decl.role().and_then(|role| role.tag()) else {
                        continue;
                    };
                    let problem = Role::from_atom(role.text())
                        .and_then(|role| interface_shape(role, decl, top));
                    (role, decl.name(), "@interface", problem)
                }
                _ => continue,
            };
            let Some(role_kind) = Role::from_atom(role.text()) else {
                continue;
            };
            let name = name.map_or_else(String::new, |name| name.text().to_owned());
            if let Some(problem) = problem {
                faults.push(Fault::at(
                    parsed.source_id,
                    role.span(),
                    format!(
                        "{}: the declaration `{name}` does not have the shape the role requires: {problem}",
                        role.text()
                    ),
                ));
            }
            claims.entry(role_kind).or_default().push(Claim {
                module: parsed.path.to_owned(),
                name,
                kind,
                source_id: parsed.source_id.to_owned(),
                span: role.span(),
            });
        }
    }
    let mut bound = BTreeMap::new();
    for role in Role::ALL {
        let mut claimed = claims.remove(&role).unwrap_or_default();
        match claimed.len() {
            0 => faults.push(Fault::bare(format!(
                "{}: no declaration claims the role",
                role.atom()
            ))),
            1 => {
                if let Some(claim) = claimed.pop() {
                    bound.insert(role, claim);
                }
            }
            count => {
                for later in claimed.iter().skip(1) {
                    faults.push(Fault::at(
                        &later.source_id,
                        later.span,
                        format!(
                            "{}: {count} declarations claim the role; `{}` of `{}` repeats it",
                            role.atom(),
                            later.name,
                            later.module
                        ),
                    ));
                }
            }
        }
    }
    (bound, faults)
}

/// Why the type `decl` does not have the shape `role` requires, or `None`
/// when it does.
fn type_shape(role: Role, decl: &TypeDecl<'_>, top: Scope<'_>) -> Option<String> {
    let parameters = decl.params();
    let scope = top.with_binders(parameters.iter().map(|parameter| parameter.text()));
    let binder = |index: usize| Written::Binder(index);
    let (arity, expected) = match role {
        Role::Bool => (0, Shape::Cases(&[("false", &[]), ("true", &[])])),
        Role::Option => (1, Shape::Cases(&[("none", &[]), ("some", &[("value", 0)])])),
        Role::Result => (
            2,
            Shape::Cases(&[("ok", &[("value", 0)]), ("err", &[("error", 1)])]),
        ),
        Role::Str => (
            0,
            Shape::Wrapper(Written::List(Box::new(Written::Core(Type::Char)))),
        ),
        Role::Bytes => (
            0,
            Shape::Wrapper(Written::List(Box::new(Written::Core(Type::U8)))),
        ),
        Role::Dict => (
            2,
            Shape::Wrapper(Written::List(Box::new(Written::Tuple(vec![
                binder(0),
                binder(1),
            ])))),
        ),
        Role::Iter | Role::Equatable | Role::Ordered => {
            return Some("the role is played by an interface, not a type".to_owned());
        }
    };
    if parameters.len() != arity {
        return Some(format!(
            "it has {} type parameters, not {arity}",
            parameters.len()
        ));
    }
    match (expected, decl.body()) {
        (Shape::Cases(expected), Some(TypeBody::Cases(cases))) => {
            let written = cases
                .iter()
                .map(|case| {
                    let fields = case
                        .fields()
                        .unwrap_or_default()
                        .iter()
                        .map(|(name, ty)| (name.text().to_owned(), scope.read(ty)))
                        .collect::<Vec<_>>();
                    (case.name().map(|name| name.text().to_owned()), fields)
                })
                .collect::<Vec<_>>();
            let fits = written.len() == expected.len()
                && written.iter().zip(expected).all(
                    |((name, fields), (case, wanted))| {
                        name.as_deref() == Some(*case)
                            && fields.len() == wanted.len()
                            && fields.iter().zip(*wanted).all(
                                |((field, ty), (want, index))| {
                                    field == want && *ty == Some(binder(*index))
                                },
                            )
                    },
                );
            (!fits).then(|| format!("its cases are not {}", spell_cases(expected)))
        }
        (Shape::Wrapper(expected), Some(TypeBody::Of(ty))) => (scope.read(&ty)
            != Some(expected))
        .then(|| "it does not wrap the representation the role requires".to_owned()),
        (Shape::Cases(expected), _) => Some(format!(
            "it is not a variant type with the cases {}",
            spell_cases(expected)
        )),
        (Shape::Wrapper(_), _) => Some(
            "it is not a wrapper over the representation the role requires".to_owned(),
        ),
    }
}

/// The cases a role requires, each with its fields as (name, binder index).
type Cases = &'static [(&'static str, &'static [(&'static str, usize)])];

enum Shape {
    Cases(Cases),
    Wrapper(Written),
}

fn spell_cases(cases: Cases) -> String {
    cases
        .iter()
        .map(|(name, fields)| {
            if fields.is_empty() {
                (*name).to_owned()
            } else {
                let fields = fields
                    .iter()
                    .map(|(field, _)| *field)
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{name} {{{fields}}}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A contract member a role requires: its name, whether it is abstract, and
/// its positional parameter types and result type.
struct Member {
    name: &'static str,
    is_abstract: bool,
    parameters: Vec<Written>,
    result: Written,
}

fn member(
    name: &'static str,
    is_abstract: bool,
    arity: usize,
    result: Written,
) -> Member {
    Member {
        name,
        is_abstract,
        parameters: vec![Written::SelfType; arity],
        result,
    }
}

/// Why the interface `decl` does not have the shape `role` requires, or
/// `None` when it does.
fn interface_shape(
    role: Role,
    decl: &InterfaceDecl<'_>,
    top: Scope<'_>,
) -> Option<String> {
    let bool_type = || Written::Library(Library::Bool, Vec::new());
    let (associated, members, defaults_open): (&[&str], Vec<Member>, bool) = match role
    {
        Role::Equatable => (&[], vec![member("equal", true, 2, bool_type())], false),
        Role::Ordered => (
            &[],
            vec![
                member(
                    "compare",
                    true,
                    2,
                    Written::Library(Library::Ordering, Vec::new()),
                ),
                member("less", false, 2, bool_type()),
                member("lessOrEqual", false, 2, bool_type()),
                member("greater", false, 2, bool_type()),
                member("greaterOrEqual", false, 2, bool_type()),
                member("min", false, 2, Written::SelfType),
                member("max", false, 2, Written::SelfType),
                member("inRange", false, 3, bool_type()),
            ],
            false,
        ),
        Role::Iter => (
            &["Item"],
            vec![Member {
                name: "next",
                is_abstract: true,
                parameters: vec![Written::SelfType],
                result: Written::Library(
                    Library::Option,
                    vec![Written::Tuple(vec![
                        Written::Associated("Item".to_owned()),
                        Written::SelfType,
                    ])],
                ),
            }],
            true,
        ),
        _ => return Some("the role is played by a type, not an interface".to_owned()),
    };
    if !decl.params().is_empty() {
        return Some("it has type parameters".to_owned());
    }
    let written_associated = decl
        .assoc_types()
        .iter()
        .map(|assoc| assoc.name().map(|name| name.text().to_owned()))
        .collect::<Vec<_>>();
    if written_associated
        != associated
            .iter()
            .map(|name| Some((*name).to_owned()))
            .collect::<Vec<_>>()
    {
        return Some(format!(
            "its associated types are not exactly [{}]",
            associated.join(" ")
        ));
    }
    let scope = top
        .with_self(Some(Written::SelfType))
        .with_associated(associated.iter().copied());
    let written = decl.members();
    for wanted in &members {
        let found = written
            .iter()
            .filter(|callable| {
                callable
                    .name()
                    .is_some_and(|name| name.text() == wanted.name)
            })
            .collect::<Vec<_>>();
        let [callable] = found.as_slice() else {
            return Some(format!(
                "it does not declare `{}` exactly once",
                wanted.name
            ));
        };
        if let Some(problem) = member_shape(callable, wanted, &scope) {
            return Some(problem);
        }
    }
    for callable in &written {
        let name = callable.name().map_or("", |name| name.text());
        let listed = members.iter().any(|wanted| wanted.name == name);
        if !(listed || defaults_open && callable.body().is_some()) {
            return Some(format!(
                "it declares the member `{name}`, which the role does not have"
            ));
        }
    }
    None
}

fn member_shape(
    callable: &Callable<'_>,
    wanted: &Member,
    scope: &Scope<'_>,
) -> Option<String> {
    let name = wanted.name;
    let is_abstract = callable.body().is_none() && callable.external().is_none();
    if wanted.is_abstract != is_abstract {
        let what = if wanted.is_abstract {
            "abstract"
        } else {
            "a default"
        };
        return Some(format!("`{name}` is not {what}"));
    }
    let takes = callable
        .takes()
        .iter()
        .map(|(_, ty)| scope.read(ty))
        .collect::<Vec<_>>();
    let result = callable.result().and_then(|(_, ty)| scope.read(&ty));
    let fits = callable.params().is_empty()
        && callable.labelled().is_empty()
        && takes.len() == wanted.parameters.len()
        && takes
            .iter()
            .zip(&wanted.parameters)
            .all(|(written, expected)| written.as_ref() == Some(expected))
        && result.as_ref() == Some(&wanted.result);
    (!fits).then(|| format!("`{name}` does not have the signature the role requires"))
}

/// The faults of the three core types: each is a variant type of `@std.core`
/// with the cases of the runtime chapter, in order.
pub(crate) fn check_core_types(
    modules: &[Parsed<'_>],
) -> (BTreeMap<Library, Claim>, Vec<Fault>) {
    let mut found = BTreeMap::new();
    let mut faults = Vec::new();
    let core = modules.iter().find(|parsed| parsed.path == CORE_MODULE);
    for (library, name) in CORE_TYPES {
        let expected: &[&str] = match library {
            Library::Ordering => &["less", "equal", "greater"],
            Library::ArithmeticError => &["overflow", "divisionByZero", "invalidShift"],
            _ => &["outOfRange", "invalidFormat", "unrepresentable"],
        };
        let declaration = core.and_then(|parsed| {
            parsed
                .module
                .declarations()
                .into_iter()
                .find_map(|declaration| match declaration {
                    Declaration::Type(decl)
                        if decl
                            .name()
                            .is_some_and(|written| written.text() == name) =>
                    {
                        Some((parsed, decl))
                    }
                    _ => None,
                })
        });
        let Some((parsed, decl)) = declaration else {
            faults.push(Fault::bare(format!(
                "the core type `{name}` is not declared in `{CORE_MODULE}`"
            )));
            continue;
        };
        let cases = match decl.body() {
            Some(TypeBody::Cases(cases)) => cases
                .iter()
                .map(|case| {
                    case.fields()
                        .is_none()
                        .then(|| case.name().map(|name| name.text().to_owned()))
                        .flatten()
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let span = decl
            .name()
            .map_or(decl.construct().span(), |name| name.span());
        if decl.params().is_empty()
            && cases.len() == expected.len()
            && cases
                .iter()
                .zip(expected)
                .all(|(case, want)| case.as_deref() == Some(*want))
        {
            found.insert(
                library,
                Claim {
                    module: parsed.path.to_owned(),
                    name: name.to_owned(),
                    kind: "@type",
                    source_id: parsed.source_id.to_owned(),
                    span,
                },
            );
        } else {
            faults.push(Fault::at(
                parsed.source_id,
                span,
                format!(
                    "the core type `{name}` is not a variant type with the cases {}",
                    expected.join(", ")
                ),
            ));
        }
    }
    (found, faults)
}
