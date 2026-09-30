//! Standard-library declarations the checker recognizes by identity.
//!
//! `docs/spec/06-runtime.md` makes lookups and `try` answer with the types
//! playing the `@option` and `@result` roles, and `@std.core` declares the
//! ordering and error types. A checking run that does not import those
//! modules still needs their types, so they are declared here from the
//! embedded modules under the identities the resolver gives them, and a run
//! that already declared one keeps its own.

use std::path::Path;
use std::sync::OnceLock;

use vibra_ir::Type;
use vibra_ir::external::{CompilerIntrinsic, RoleTypes};
use vibra_syntax::{
    Declaration, DeftypeBody, DeftypeDeclaration, SourceAst, TypeMember,
};

use crate::nominal::TypeNames;
use crate::stdlib::{
    STDLIB_BUILTIN_SOURCE_ID, STDLIB_CORE_SOURCE_ID, STDLIB_OPTION_SOURCE_ID,
    STDLIB_RESULT_SOURCE_ID, embedded_module, stdlib_type_id,
};

/// The embedded modules that declare types every run can reach: by path,
/// source identity, and module name under `@std`.
const TYPE_MODULES: [(&str, &str, &str); 3] = [
    ("std/option.vib", STDLIB_OPTION_SOURCE_ID, "option"),
    ("std/result.vib", STDLIB_RESULT_SOURCE_ID, "result"),
    ("std/core.vib", STDLIB_CORE_SOURCE_ID, "core"),
];

fn type_modules() -> &'static [(&'static str, &'static str, SourceAst)] {
    static MODULES: OnceLock<Vec<(&'static str, &'static str, SourceAst)>> =
        OnceLock::new();
    MODULES.get_or_init(|| {
        TYPE_MODULES
            .iter()
            .filter_map(|(path, source_id, module)| {
                let text = std::str::from_utf8(embedded_module(path)?).ok()?;
                let document =
                    vibra_syntax::parse_source(Path::new(source_id), text).ok()?;
                Some((*source_id, *module, document.ast()?.clone()))
            })
            .collect()
    })
}

/// Declares every type of `@std.option`, `@std.result`, and `@std.core` that
/// the run has not declared itself, adding each to `declarations` so its body
/// lowers with the run's own declarations. A role-playing type the run
/// already declared keeps the run's declaration; the compiler knows each role
/// type only through the `role:` it claims.
pub(crate) fn declare_standard_types<'a>(
    types: &mut TypeNames,
    declarations: &mut Vec<(usize, &'a DeftypeDeclaration)>,
) where
    'static: 'a,
{
    // `@std.builtin` names `option` through its own import.
    types.import(STDLIB_BUILTIN_SOURCE_ID, "option", STDLIB_OPTION_SOURCE_ID);
    for (source_id, module, ast) in type_modules() {
        for declaration in ast.declarations() {
            let Declaration::Deftype(declaration) = declaration else {
                continue;
            };
            let id = stdlib_type_id(&[module], declaration.name().value());
            let role_claimed =
                declaration.attributes().items().iter().any(|attribute| {
                    matches!(attribute, vibra_syntax::Attribute::Role(role)
                    if types.role(role.value()).is_some())
                });
            if role_claimed || types.index_of(&id).is_some() {
                continue;
            }
            let index = types.declare(source_id, declaration, id);
            declarations.push((index, declaration));
        }
    }
}

/// `(option value)` over the type playing `@option`, when one does.
pub(crate) fn option_of(types: &TypeNames, value: Type) -> Option<Type> {
    let index = types.role("option")?;
    Some(Type::Applied(types.get(index)?.id.clone(), vec![value]))
}

/// The role-playing types a registry signature may name.
pub(crate) fn role_types(types: &TypeNames) -> RoleTypes {
    RoleTypes::new(
        types
            .role("option")
            .and_then(|index| types.get(index))
            .map(|declared| declared.id.clone()),
    )
}

fn builtin_module() -> Option<&'static SourceAst> {
    static MODULE: OnceLock<Option<SourceAst>> = OnceLock::new();
    MODULE
        .get_or_init(|| {
            let bytes = embedded_module("std/builtin.vib")?;
            let text = std::str::from_utf8(bytes).ok()?;
            let document =
                vibra_syntax::parse_source(Path::new(STDLIB_BUILTIN_SOURCE_ID), text)
                    .ok()?;
            document.ast().cloned()
        })
        .as_ref()
}

/// The builtin types `@std.builtin` declares with `intrinsic-type`, with the
/// type each denotes over its generic parameters.
fn builtin_self_type(atom: &str, parameters: &[String]) -> Option<Type> {
    let param =
        |index: usize| parameters.get(index).map(|name| Type::Param(name.clone()));
    match (atom, parameters.len()) {
        ("array", 1) => Some(Type::Array(Box::new(param(0)?))),
        ("map", 2) => Some(Type::Map(Box::new(param(0)?), Box::new(param(1)?))),
        _ => None,
    }
}

/// One static method of a builtin type, bound to a compiler operation.
#[derive(Clone, Debug)]
pub(crate) struct BuiltinMember {
    /// The builtin type name, such as `array`.
    pub(crate) type_name: String,
    /// The member name, such as `of`.
    pub(crate) member: String,
    /// The checked signature, exactly the registry's.
    pub(crate) signature: vibra_ir::FunctionSignature,
    /// The bound registry operation.
    pub(crate) intrinsic: CompilerIntrinsic,
    /// The complete generic parameter list: the type's, then the member's.
    pub(crate) type_parameters: Vec<String>,
}

impl BuiltinMember {
    /// The value path that reaches the member, such as `array.of`.
    pub(crate) fn path(&self) -> String {
        format!("{}.{}", self.type_name, self.member)
    }
}

/// The `(type, member)` names of every builtin static method, for a
/// resolver that needs to recognize the paths before types are lowered.
#[must_use]
pub fn builtin_member_names() -> Vec<(String, String)> {
    builtin_module()
        .into_iter()
        .flat_map(|ast| ast.declarations())
        .filter_map(|declaration| match declaration {
            Declaration::Deftype(value) => Some(value),
            _ => None,
        })
        .flat_map(|value| {
            value
                .members()
                .iter()
                .filter_map(move |member| match member {
                    TypeMember::Method(method) => Some((
                        value.name().value().to_owned(),
                        method.name().value().to_owned(),
                    )),
                    TypeMember::Implementation(_) => None,
                })
        })
        .collect()
}

/// Every builtin static method whose declaration lowers to exactly its
/// registry signature. The embedded module is fixed at build time, so a
/// member that fails to bind is a toolchain defect a host test catches.
pub(crate) fn builtin_members(types: &TypeNames) -> Vec<BuiltinMember> {
    let Some(ast) = builtin_module() else {
        return Vec::new();
    };
    let mut members = Vec::new();
    for declaration in ast.declarations() {
        let Declaration::Deftype(value) = declaration else {
            continue;
        };
        let DeftypeBody::Intrinsic(atom) = value.body() else {
            continue;
        };
        if atom.value() != value.name().value() {
            continue;
        }
        let owner = crate::nominal::generic_names(value.attributes().items());
        let Some(self_type) = builtin_self_type(atom.value(), &owner) else {
            continue;
        };
        for member in value.members() {
            let TypeMember::Method(method) = member else {
                continue;
            };
            let mut generics = owner.clone();
            generics.extend(crate::nominal::generic_names(method.attributes().items()));
            let mut ignored = Vec::new();
            let Some(signature) = crate::check_signature(
                STDLIB_BUILTIN_SOURCE_ID,
                method,
                &mut ignored,
                types,
                crate::nominal::Scope::new(Some(&self_type), &generics),
            ) else {
                continue;
            };
            let Some(intrinsic) = crate::compiler_intrinsic(
                STDLIB_BUILTIN_SOURCE_ID,
                method,
                &mut ignored,
                true,
                &signature,
                &role_types(types),
            ) else {
                continue;
            };
            if !ignored.is_empty()
                || !signature.same_shape(&intrinsic.signature(&role_types(types)))
                || generics != intrinsic.type_parameters()
            {
                continue;
            }
            members.push(BuiltinMember {
                type_name: value.name().value().to_owned(),
                member: method.name().value().to_owned(),
                signature,
                intrinsic,
                type_parameters: generics,
            });
        }
    }
    members
}

/// Every `(role, type name)` the embedded standard library claims with
/// `role:`, for a resolver that recognizes the role vocabulary before types
/// are lowered.
#[must_use]
pub fn role_type_names() -> Vec<(String, String)> {
    static ROLES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    ROLES
        .get_or_init(|| {
            let mut roles = Vec::new();
            for (path, bytes) in crate::stdlib::embedded_modules() {
                let Ok(text) = std::str::from_utf8(bytes) else {
                    continue;
                };
                let source_id = format!("stdlib/src/{path}");
                let Ok(document) =
                    vibra_syntax::parse_source(Path::new(&source_id), text)
                else {
                    continue;
                };
                let Some(ast) = document.ast() else {
                    continue;
                };
                for declaration in ast.declarations() {
                    let Declaration::Deftype(value) = declaration else {
                        continue;
                    };
                    for attribute in value.attributes().items() {
                        if let vibra_syntax::Attribute::Role(role) = attribute {
                            roles.push((
                                role.value().to_owned(),
                                value.name().value().to_owned(),
                            ));
                        }
                    }
                }
            }
            roles
        })
        .clone()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{builtin_member_names, builtin_members, declare_standard_types};
    use crate::nominal::TypeNames;

    #[test]
    fn every_builtin_member_binds_its_registry_operation() {
        let mut types = TypeNames::default();
        let mut declarations = Vec::new();
        declare_standard_types(&mut types, &mut declarations);
        let mut diagnostics = Vec::new();
        types.lower_bodies(&declarations, &mut diagnostics);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let bound = builtin_members(&types)
            .iter()
            .map(super::BuiltinMember::path)
            .collect::<Vec<_>>();
        let declared = builtin_member_names()
            .into_iter()
            .map(|(type_name, member)| format!("{type_name}.{member}"))
            .collect::<Vec<_>>();
        assert_eq!(bound, declared);
        assert_eq!(declared.len(), 6);
    }
}
