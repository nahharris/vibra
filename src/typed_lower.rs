//! Declaration and signature lowering from typed S-expression modules.
//!
//! This index is deliberately non-executable. Full body lowering remains on
//! the legacy path until typed expressions can produce validated bodies.

use crate::ast::{
    Annotation, AnnotationKind, Definition, DocumentId, Function, ImplItem, MethodBinding, Module,
    TestMeta, TopLevel, TypeExpr, TypeExprKind, Visibility,
};
use crate::lower::{ImplBody, ImplKey, ImplMethodBinding, TypeAlias, TypeRef};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypedModuleIdentity {
    pub alias: String,
    pub document: DocumentId,
}

#[derive(Debug, Clone, Copy)]
pub struct TypedModuleInput<'a> {
    pub alias: &'a str,
    pub module: &'a Module,
}

#[derive(Debug, Clone)]
pub struct TypedFunctionSignature {
    pub alias: String,
    pub symbol: String,
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<Vec<TypeRef>>,
    pub arg_names: Vec<String>,
    pub arg_types: Vec<TypeRef>,
    pub return_type: TypeRef,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TypedSignatureIndex {
    pub imports: BTreeMap<(TypedModuleIdentity, String), String>,
    pub visibility: BTreeMap<String, Visibility>,
    pub aliases: HashMap<String, TypeAlias>,
    pub functions: HashMap<String, TypedFunctionSignature>,
    pub constants: HashMap<String, TypeRef>,
    pub tests: HashMap<String, TypedTestSignature>,
    pub impls: HashMap<ImplKey, ImplBody>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedTestSignature {
    pub module: TypedModuleIdentity,
    pub profile: String,
    pub tags: Vec<String>,
}

#[derive(Default)]
struct DeclarationAnnotations<'a> {
    doc: Option<String>,
    parameters: Vec<String>,
    bounds: Vec<Vec<TypeRef>>,
    definitions: Vec<&'a Function>,
    implementations: Vec<&'a AnnotationKind>,
}

pub fn lower_typed_signatures<'a>(
    modules: impl IntoIterator<Item = TypedModuleInput<'a>>,
) -> Result<TypedSignatureIndex> {
    let modules: Vec<_> = modules.into_iter().collect();
    let mut identities = BTreeSet::new();
    let mut declared_aliases = BTreeSet::new();
    for input in &modules {
        let identity = identity(input);
        if !identities.insert(identity.clone()) {
            bail!(
                "duplicate typed module identity `{}` ({})",
                identity.alias,
                identity.document.raw()
            );
        }
        for form in &input.module.forms {
            if let TopLevel::Definition(definition) = form {
                declared_aliases.insert(qualify(input.alias, &definition.name.value));
            }
        }
    }

    let mut index = TypedSignatureIndex {
        imports: BTreeMap::new(),
        visibility: BTreeMap::new(),
        aliases: HashMap::new(),
        functions: HashMap::new(),
        constants: HashMap::new(),
        tests: HashMap::new(),
        impls: HashMap::new(),
    };
    for input in modules {
        lower_module(input, &declared_aliases, &mut index)?;
    }
    Ok(index)
}

fn lower_module(
    input: TypedModuleInput<'_>,
    declared_aliases: &BTreeSet<String>,
    index: &mut TypedSignatureIndex,
) -> Result<()> {
    let module_identity = identity(&input);
    for form in &input.module.forms {
        match form {
            TopLevel::Import(import) => {
                let key = (module_identity.clone(), import.alias.value.clone());
                if index
                    .imports
                    .insert(key, import.path.value.clone())
                    .is_some()
                {
                    bail!(
                        "duplicate typed import alias `{}` in `{}`",
                        import.alias.value,
                        input.alias
                    );
                }
            }
            TopLevel::Definition(definition) => {
                lower_definition(input.alias, definition, declared_aliases, index)?
            }
            TopLevel::Function(function) => {
                lower_function(input.alias, function, "", &[], &[], declared_aliases, index)?;
                insert_visibility(
                    index,
                    qualify(input.alias, &function.name.value),
                    function.visibility,
                )?;
            }
            TopLevel::Constant(constant) => {
                let annotations = annotations(
                    &constant.annotations,
                    &BTreeSet::new(),
                    input.alias,
                    declared_aliases,
                )?;
                let generics = annotations.parameters.iter().cloned().collect();
                let ty = lower_type(&constant.ty, &generics, input.alias, declared_aliases)?;
                let key = qualify(input.alias, &constant.name.value);
                if index.constants.insert(key.clone(), ty).is_some() {
                    bail!("duplicate typed constant `{key}`");
                }
                insert_visibility(index, key, constant.visibility)?;
            }
            TopLevel::Test(test) => {
                let tags = test
                    .metadata
                    .iter()
                    .find_map(|metadata| match metadata {
                        TestMeta::Tags(tags) => {
                            Some(tags.iter().map(|tag| tag.value.clone()).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let key = qualify(input.alias, &test.name.value);
                if index
                    .tests
                    .insert(
                        key.clone(),
                        TypedTestSignature {
                            module: module_identity.clone(),
                            profile: test.profile.value.clone(),
                            tags,
                        },
                    )
                    .is_some()
                {
                    bail!("duplicate typed test `{key}`");
                }
            }
            TopLevel::Macro(_) => {}
        }
    }
    Ok(())
}

fn lower_definition(
    module_alias: &str,
    definition: &Definition,
    declared_aliases: &BTreeSet<String>,
    index: &mut TypedSignatureIndex,
) -> Result<()> {
    let annotations = annotations(
        &definition.annotations,
        &BTreeSet::new(),
        module_alias,
        declared_aliases,
    )?;
    let generics: BTreeSet<_> = annotations.parameters.iter().cloned().collect();
    let key = qualify(module_alias, &definition.name.value);
    let body = match lower_type(&definition.body, &generics, module_alias, declared_aliases)? {
        TypeRef::Newtype { inner, .. } => TypeRef::Newtype {
            name: key.clone(),
            inner,
        },
        body => body,
    };
    if index
        .aliases
        .insert(
            key.clone(),
            TypeAlias {
                alias: module_alias.to_string(),
                name: definition.name.value.clone(),
                type_params: annotations.parameters.clone(),
                type_param_bounds: annotations.bounds.clone(),
                body,
                doc: annotations.doc.clone(),
            },
        )
        .is_some()
    {
        bail!("duplicate typed type definition `{key}`");
    }
    insert_visibility(index, key.clone(), definition.visibility)?;

    for function in annotations.definitions {
        lower_function(
            module_alias,
            function,
            &definition.name.value,
            &annotations.parameters,
            &annotations.bounds,
            declared_aliases,
            index,
        )?;
    }
    for implementation in annotations.implementations {
        lower_implementation(
            module_alias,
            &key,
            implementation,
            &annotations.parameters,
            &annotations.bounds,
            declared_aliases,
            index,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_function(
    module_alias: &str,
    function: &Function,
    owner: &str,
    inherited_parameters: &[String],
    inherited_bounds: &[Vec<TypeRef>],
    declared_aliases: &BTreeSet<String>,
    index: &mut TypedSignatureIndex,
) -> Result<String> {
    let inherited_generics = inherited_parameters.iter().cloned().collect();
    let annotations = annotations(
        &function.annotations,
        &inherited_generics,
        module_alias,
        declared_aliases,
    )?;
    let mut generics = inherited_generics;
    generics.extend(annotations.parameters.iter().cloned());
    let symbol = if owner.is_empty() {
        function.name.value.clone()
    } else {
        format!("{owner}.{}", function.name.value)
    };
    let key = qualify(module_alias, &symbol);
    let mut type_params = inherited_parameters.to_vec();
    type_params.extend(annotations.parameters.iter().cloned());
    let mut type_param_bounds = inherited_bounds.to_vec();
    type_param_bounds.extend(annotations.bounds.iter().cloned());
    let signature = TypedFunctionSignature {
        alias: module_alias.to_string(),
        symbol,
        type_params,
        type_param_bounds,
        arg_names: function
            .parameters
            .iter()
            .map(|parameter| parameter.name.value.clone())
            .collect(),
        arg_types: function
            .parameters
            .iter()
            .map(|parameter| lower_type(&parameter.ty, &generics, module_alias, declared_aliases))
            .collect::<Result<_>>()?,
        return_type: lower_type(
            &function.return_type,
            &generics,
            module_alias,
            declared_aliases,
        )?,
        doc: annotations.doc,
    };
    if index.functions.insert(key.clone(), signature).is_some() {
        bail!("duplicate typed function signature `{key}`");
    }
    Ok(key)
}

#[allow(clippy::too_many_arguments)]
fn lower_implementation(
    module_alias: &str,
    implementing_type: &str,
    annotation: &AnnotationKind,
    generic_parameters: &[String],
    generic_bounds: &[Vec<TypeRef>],
    declared_aliases: &BTreeSet<String>,
    index: &mut TypedSignatureIndex,
) -> Result<()> {
    let AnnotationKind::Implementation { interface, items } = annotation else {
        return Ok(());
    };
    let generics = generic_parameters.iter().cloned().collect();
    let lowered_interface = lower_type(interface, &generics, module_alias, declared_aliases)?;
    let (interface_name, direct_args) = named_application(&lowered_interface)?;
    let type_items = items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Types(types) => Some(types),
            _ => None,
        })
        .collect::<Vec<_>>();
    if type_items.len() > 1 {
        bail!("typed implementation `{interface_name}` repeats `types:`");
    }
    if !direct_args.is_empty() && !type_items.is_empty() {
        bail!(
            "typed implementation `{interface_name}` cannot combine direct interface arguments with `types:`"
        );
    }
    let interface_args = if let Some(types) = type_items.first() {
        types
            .iter()
            .map(|ty| lower_type(ty, &generics, module_alias, declared_aliases))
            .collect::<Result<_>>()?
    } else {
        direct_args
    };
    let mut methods = HashMap::new();
    for item in items {
        let ImplItem::Method { name, binding, .. } = item else {
            continue;
        };
        let binding = match binding {
            MethodBinding::Reference(reference) => {
                ImplMethodBinding::Alias(reference.value.clone())
            }
            MethodBinding::Function(function) => {
                let owner = format!(
                    "{}.{}",
                    unqualify(module_alias, implementing_type),
                    unqualify(module_alias, &interface_name)
                );
                ImplMethodBinding::Fresh(lower_function(
                    module_alias,
                    function,
                    &owner,
                    generic_parameters,
                    generic_bounds,
                    declared_aliases,
                    index,
                )?)
            }
        };
        if methods.insert(name.value.clone(), binding).is_some() {
            bail!(
                "typed implementation `{interface_name}` repeats method `{}`",
                name.value
            );
        }
    }
    let key = ImplKey {
        implementing_type: implementing_type.to_string(),
        interface: interface_name,
    };
    if index
        .impls
        .insert(
            key.clone(),
            ImplBody {
                methods,
                interface_args,
                impl_type_params: generic_parameters.to_vec(),
            },
        )
        .is_some()
    {
        bail!(
            "duplicate typed implementation `{}` for `{}`",
            key.interface,
            key.implementing_type
        );
    }
    Ok(())
}

fn annotations<'a>(
    annotations: &'a [Annotation],
    inherited_generics: &BTreeSet<String>,
    module_alias: &str,
    declared_aliases: &BTreeSet<String>,
) -> Result<DeclarationAnnotations<'a>> {
    let mut result = DeclarationAnnotations::default();
    let mut generics = inherited_generics.clone();
    for annotation in annotations {
        if let AnnotationKind::Where(parameters) = &annotation.value {
            for parameter in parameters {
                if !generics.insert(parameter.name.value.clone()) {
                    bail!("duplicate typed type parameter `{}`", parameter.name.value);
                }
                result.parameters.push(parameter.name.value.clone());
            }
        }
    }
    for annotation in annotations {
        match &annotation.value {
            AnnotationKind::Doc(doc) => result.doc = Some(doc.clone()),
            AnnotationKind::Where(parameters) => {
                result.bounds = parameters
                    .iter()
                    .map(|parameter| {
                        parameter
                            .bounds
                            .iter()
                            .map(|bound| {
                                lower_type(bound, &generics, module_alias, declared_aliases)
                            })
                            .collect()
                    })
                    .collect::<Result<_>>()?;
            }
            AnnotationKind::Definitions(functions) => result.definitions.extend(functions),
            implementation @ AnnotationKind::Implementation { .. } => {
                result.implementations.push(implementation)
            }
        }
    }
    Ok(result)
}

fn lower_type(
    ty: &TypeExpr,
    generics: &BTreeSet<String>,
    module_alias: &str,
    declared_aliases: &BTreeSet<String>,
) -> Result<TypeRef> {
    let lower = |ty| lower_type(ty, generics, module_alias, declared_aliases);
    Ok(match &ty.value {
        TypeExprKind::Named(name) if generics.contains(name) => TypeRef::Generic(name.clone()),
        TypeExprKind::Named(name) => named_type(name, module_alias, declared_aliases),
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            if generics.contains(&constructor.value) {
                bail!(
                    "generic type parameter `{}` cannot head a type application",
                    constructor.value
                );
            }
            TypeRef::Instantiated {
                base: named_key(&constructor.value, module_alias, declared_aliases),
                type_args: arguments.iter().map(lower).collect::<Result<_>>()?,
            }
        }
        TypeExprKind::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|field| Ok((field.name.value.clone(), lower(&field.ty)?)))
                .collect::<Result<_>>()?,
        ),
        TypeExprKind::Tuple(items) => {
            TypeRef::Tuple(items.iter().map(lower).collect::<Result<_>>()?)
        }
        TypeExprKind::Array(item) => TypeRef::Array(Box::new(lower(item)?)),
        TypeExprKind::Map(key, value) => TypeRef::Map {
            key: Box::new(lower(key)?),
            value: Box::new(lower(value)?),
        },
        TypeExprKind::Union(items) => {
            TypeRef::Union(items.iter().map(lower).collect::<Result<_>>()?)
        }
        TypeExprKind::Enum(variants) => TypeRef::Enum(
            variants
                .iter()
                .map(|variant| Ok((variant.name.value.clone(), lower(&variant.ty)?)))
                .collect::<Result<_>>()?,
        ),
        TypeExprKind::Interface(methods) => TypeRef::Interface(
            methods
                .iter()
                .map(|method| Ok((method.name.value.clone(), lower(&method.ty)?)))
                .collect::<Result<_>>()?,
        ),
        TypeExprKind::Function { parameters, result } => TypeRef::FnType {
            args: Box::new(TypeRef::Tuple(
                parameters.iter().map(lower).collect::<Result<_>>()?,
            )),
            return_type: Box::new(lower(result)?),
        },
        TypeExprKind::Newtype(inner) => TypeRef::Newtype {
            name: String::new(),
            inner: Box::new(lower(inner)?),
        },
        TypeExprKind::Mutable(inner) => TypeRef::Mutable(Box::new(lower(inner)?)),
        TypeExprKind::Reference(inner) => TypeRef::Reference {
            inner: Box::new(lower(inner)?),
            mutable: false,
        },
        TypeExprKind::Intersect(items) => {
            TypeRef::Intersect(items.iter().map(lower).collect::<Result<_>>()?)
        }
        TypeExprKind::Domain { head, .. } => {
            bail!(
                "typed signature lowering does not yet support domain type `{}`",
                head.value
            )
        }
    })
}

fn named_application(ty: &TypeRef) -> Result<(String, Vec<TypeRef>)> {
    match ty {
        TypeRef::Named(name) => Ok((name.clone(), Vec::new())),
        TypeRef::Instantiated { base, type_args } => Ok((base.clone(), type_args.clone())),
        _ => bail!("typed implementation interface must be a named type"),
    }
}

fn named_type(name: &str, module_alias: &str, declared_aliases: &BTreeSet<String>) -> TypeRef {
    match name {
        "bool" => TypeRef::Bool,
        "str" => TypeRef::Str,
        "int8" => TypeRef::Int8,
        "int16" => TypeRef::Int16,
        "int32" => TypeRef::Int32,
        "int64" => TypeRef::Int64,
        "uint8" => TypeRef::UInt8,
        "uint16" => TypeRef::UInt16,
        "uint32" => TypeRef::UInt32,
        "uint64" => TypeRef::UInt64,
        "float32" => TypeRef::Float32,
        "float64" => TypeRef::Float64,
        "void" | "unit" => TypeRef::Void,
        "self" => TypeRef::SelfType,
        other => TypeRef::Named(named_key(other, module_alias, declared_aliases)),
    }
}

fn named_key(name: &str, module_alias: &str, declared_aliases: &BTreeSet<String>) -> String {
    let local = qualify(module_alias, name);
    if declared_aliases.contains(&local) {
        local
    } else {
        name.to_string()
    }
}

fn identity(input: &TypedModuleInput<'_>) -> TypedModuleIdentity {
    TypedModuleIdentity {
        alias: input.alias.to_string(),
        document: input.module.document_id,
    }
}

fn qualify(module_alias: &str, name: &str) -> String {
    if module_alias.is_empty() {
        name.to_string()
    } else {
        format!("{module_alias}.{name}")
    }
}

fn unqualify<'a>(module_alias: &str, name: &'a str) -> &'a str {
    name.strip_prefix(module_alias)
        .and_then(|name| name.strip_prefix('.'))
        .unwrap_or(name)
}

fn insert_visibility(
    index: &mut TypedSignatureIndex,
    key: String,
    visibility: Visibility,
) -> Result<()> {
    if index.visibility.insert(key.clone(), visibility).is_some() {
        bail!("duplicate typed declaration visibility `{key}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ast, syntax};

    fn module(source: &str, document_id: u64) -> Module {
        let document = syntax::parse(source).unwrap();
        ast::lower_document_with_id(&document, DocumentId::from_raw(document_id)).unwrap()
    }

    #[test]
    fn lowers_identity_bearing_modules_and_direct_applications_without_yaml() {
        let entry = module(
            r#"(import model "./model.vibra")
(fn unwrap ((input (option int64))) int64
  (do (return 0))
  doc: "Typed signature.")"#,
            1,
        );
        let model = module("(def item (record (id int64)))", 2);
        let index = lower_typed_signatures([
            TypedModuleInput {
                alias: "",
                module: &entry,
            },
            TypedModuleInput {
                alias: "model",
                module: &model,
            },
        ])
        .unwrap();
        let signature = &index.functions["unwrap"];
        assert_eq!(
            signature.arg_types,
            vec![TypeRef::Instantiated {
                base: "option".into(),
                type_args: vec![TypeRef::Int64],
            }]
        );
        assert_eq!(signature.doc.as_deref(), Some("Typed signature."));
        assert!(index.aliases.contains_key("model.item"));
        assert_eq!(index.aliases["model.item"].alias, "model");
        assert_eq!(
            index.imports[&(
                TypedModuleIdentity {
                    alias: "".into(),
                    document: DocumentId::from_raw(1),
                },
                "model".into(),
            )],
            "./model.vibra"
        );
    }

    #[test]
    fn registers_labels_defs_and_impls_as_non_executable_signatures() {
        let source = module(
            r#"(def box (record (value t))
  where: ((t comparable))
  doc: "A box."
  defs: ((fn get ((input self)) t (do (return unit))))
  impls: ((impl display
    types: (t)
    methods: ((method show display.show)))))
(private (const limit int64 10))
(test works core (do unit) tags: (fast typed))"#,
            3,
        );
        let index = lower_typed_signatures([TypedModuleInput {
            alias: "pkg",
            module: &source,
        }])
        .unwrap();
        assert_eq!(index.aliases["pkg.box"].type_params, ["t"]);
        assert!(index.functions.contains_key("pkg.box.get"));
        assert!(index.impls.contains_key(&ImplKey {
            implementing_type: "pkg.box".into(),
            interface: "display".into(),
        }));
        assert_eq!(index.constants["pkg.limit"], TypeRef::Int64);
        assert_eq!(index.tests["pkg.works"].tags, ["fast", "typed"]);
        assert_eq!(index.visibility["pkg.limit"], Visibility::Private);
    }

    #[test]
    fn rejects_ambiguous_generic_heads_and_impl_type_arguments() {
        let generic_head = module(
            "(fn bad ((value (t int64))) int64 (do (return 0)) where: ((t)))",
            4,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &generic_head,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("cannot head"));

        let mixed_impl = module(
            r#"(def box (record (value int64))
  impls: ((impl (display int64)
    types: (int64)
    methods: ((method show display.show)))))"#,
            5,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &mixed_impl,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("cannot combine"));
    }
}
