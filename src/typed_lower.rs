//! Declaration and signature lowering from typed S-expression modules.
//!
//! This index is deliberately non-executable. Full body lowering remains on
//! the legacy path until typed expressions can produce validated bodies.

use crate::ast::{
    Annotation, AnnotationKind, Definition, DocumentId, Function, ImplItem, MethodBinding, Module,
    TestMeta, TopLevel, TypeExpr, TypeExprKind, Visibility,
};
use crate::lower::{HandleAccess, ImplBody, ImplKey, ImplMethodBinding, TypeAlias, TypeRef};
use crate::type_semantics;
use anyhow::{bail, Context, Result};
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
    resolve_impl_method_references(&mut index)?;
    validate_index(&index)?;
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
        let function_key = lower_function(
            module_alias,
            function,
            &definition.name.value,
            &annotations.parameters,
            &annotations.bounds,
            declared_aliases,
            index,
        )?;
        insert_visibility(index, function_key, definition.visibility)?;
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

pub(crate) fn lower_type(
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
        TypeExprKind::Reference(inner) => match &inner.value {
            TypeExprKind::Mutable(value) => {
                if matches!(
                    value.value,
                    TypeExprKind::Mutable(_)
                        | TypeExprKind::Reference(_)
                        | TypeExprKind::MutableReference(_)
                ) {
                    bail!("typed `ref (mut ...)` cannot contain another mutable or reference type");
                }
                TypeRef::Reference {
                    inner: Box::new(lower(value)?),
                    mutable: true,
                }
            }
            TypeExprKind::Reference(_) | TypeExprKind::MutableReference(_) => {
                bail!("typed `ref` cannot wrap another reference type")
            }
            _ => TypeRef::Reference {
                inner: Box::new(lower(inner)?),
                mutable: false,
            },
        },
        TypeExprKind::MutableReference(inner) => {
            if matches!(
                inner.value,
                TypeExprKind::Mutable(_)
                    | TypeExprKind::Reference(_)
                    | TypeExprKind::MutableReference(_)
            ) {
                bail!("typed `mut-ref` cannot wrap another mutable or reference type");
            }
            TypeRef::Reference {
                inner: Box::new(lower(inner)?),
                mutable: true,
            }
        }
        TypeExprKind::Intersect(items) => {
            TypeRef::Intersect(items.iter().map(lower).collect::<Result<_>>()?)
        }
        TypeExprKind::Handle(access) => TypeRef::HostHandle(match access.value {
            crate::ast::HandleAccess::Read => HandleAccess::Read,
            crate::ast::HandleAccess::Write => HandleAccess::Write,
            crate::ast::HandleAccess::ReadWrite => HandleAccess::ReadWrite,
            crate::ast::HandleAccess::Process => HandleAccess::Process,
        }),
        TypeExprKind::WasmValue(name) => match name.value.as_str() {
            "i32" => TypeRef::Int32,
            "i64" => TypeRef::Int64,
            "f32" => TypeRef::Float32,
            "f64" => TypeRef::Float64,
            other => bail!("typed `wasm` type must name an ABI scalar, got `{other}`"),
        },
    })
}

fn resolve_impl_method_references(index: &mut TypedSignatureIndex) -> Result<()> {
    let functions = &index.functions;
    let visibility = &index.visibility;
    let imports = &index.imports;
    for (key, implementation) in &mut index.impls {
        let owner_alias = index
            .aliases
            .get(&key.implementing_type)
            .map(|owner| owner.alias.as_str())
            .unwrap_or_default();
        for binding in implementation.methods.values_mut() {
            let ImplMethodBinding::Alias(reference) = binding else {
                continue;
            };
            let local = qualify(owner_alias, reference);
            let resolved = if functions.contains_key(&local) {
                local
            } else if functions.contains_key(reference) {
                reference.clone()
            } else {
                bail!("E-IMPL-006: impl method references unknown function `{reference}`");
            };
            let target = &functions[&resolved];
            if target.alias != owner_alias {
                let import_alias = resolved.split('.').next().unwrap_or(&resolved);
                let imported = imports
                    .keys()
                    .any(|(module, alias)| module.alias == owner_alias && alias == import_alias);
                if !imported {
                    bail!(
                        "E-IMPL-006: referenced method `{resolved}` is outside the import scope of `{owner_alias}`"
                    );
                }
                if visibility.get(&resolved) != Some(&Visibility::Public) {
                    bail!("E-IMPL-006: referenced method `{resolved}` is not publicly visible");
                }
            }
            *reference = resolved;
        }
    }
    Ok(())
}

fn validate_index(index: &TypedSignatureIndex) -> Result<()> {
    for (key, alias) in &index.aliases {
        validate_bounds(key, &alias.type_params, &alias.type_param_bounds, index)?;
        validate_type(
            &alias.body,
            &alias.type_params,
            &alias.type_param_bounds,
            key,
            index,
        )?;
    }
    for (key, function) in &index.functions {
        validate_bounds(
            key,
            &function.type_params,
            &function.type_param_bounds,
            index,
        )?;
        for ty in function.arg_types.iter().chain([&function.return_type]) {
            validate_type(
                ty,
                &function.type_params,
                &function.type_param_bounds,
                key,
                index,
            )?;
        }
    }
    for (key, ty) in &index.constants {
        validate_type(ty, &[], &[], key, index)?;
    }
    for (key, implementation) in &index.impls {
        let Some(owner) = index.aliases.get(&key.implementing_type) else {
            bail!(
                "E-IMPL-007: implementation owner `{}` is not a local type",
                key.implementing_type
            );
        };
        let Some(interface) = index.aliases.get(&key.interface) else {
            bail!("E-IMPL-002: unknown typed interface `{}`", key.interface);
        };
        let TypeRef::Interface(required_methods) = &interface.body else {
            bail!(
                "E-IMPL-002: typed implementation target `{}` is not an interface",
                key.interface
            );
        };
        if implementation.interface_args.len() != interface.type_params.len() {
            bail!(
                "E-TYPE-ARITY: interface `{}` expects {} type arguments, got {}",
                key.interface,
                interface.type_params.len(),
                implementation.interface_args.len()
            );
        }
        for (position, argument) in implementation.interface_args.iter().enumerate() {
            for required in interface
                .type_param_bounds
                .get(position)
                .into_iter()
                .flatten()
            {
                if !satisfies_bound(
                    argument,
                    required,
                    &implementation.impl_type_params,
                    &owner.type_param_bounds,
                    index,
                ) {
                    bail!(
                        "E-BOUND-001: interface argument `{argument:?}` for `{}.{}` does not satisfy `{required:?}`",
                        key.interface,
                        interface.type_params[position]
                    );
                }
            }
        }
        let actual_methods: BTreeSet<_> = implementation.methods.keys().cloned().collect();
        let expected_methods: BTreeSet<_> = required_methods.keys().cloned().collect();
        if actual_methods != expected_methods {
            bail!(
                "E-IMPL-003: implementation `{}` for `{}` must bind methods {:?}, got {:?}",
                key.interface,
                key.implementing_type,
                expected_methods,
                actual_methods
            );
        }
        // Implementations are attached to a declaration, so the owner side of
        // the orphan rule must belong to the declaring module.
        if !key
            .implementing_type
            .strip_prefix(&owner.alias)
            .is_some_and(|suffix| owner.alias.is_empty() || suffix.starts_with('.'))
        {
            bail!(
                "E-IMPL-007: implementation owner `{}` is foreign to module `{}`",
                key.implementing_type,
                owner.alias
            );
        }
        for ty in &implementation.interface_args {
            validate_type(
                ty,
                &implementation.impl_type_params,
                &owner.type_param_bounds,
                &format!("impl {} for {}", key.interface, key.implementing_type),
                index,
            )?;
        }
        let substitutions: HashMap<_, _> = interface
            .type_params
            .iter()
            .cloned()
            .zip(implementation.interface_args.iter().cloned())
            .collect();
        let self_type = if owner.type_params.is_empty() {
            TypeRef::Named(key.implementing_type.clone())
        } else {
            TypeRef::Instantiated {
                base: key.implementing_type.clone(),
                type_args: owner
                    .type_params
                    .iter()
                    .cloned()
                    .map(TypeRef::Generic)
                    .collect(),
            }
        };
        for (method, expected) in required_methods {
            let TypeRef::FnType { args, return_type } =
                type_semantics::substitute(expected, &substitutions, Some(&self_type))
            else {
                bail!(
                    "E-IMPL-005: interface method `{}.{method}` is not a function type",
                    key.interface
                );
            };
            let binding = &implementation.methods[method];
            let signature_key = match binding {
                ImplMethodBinding::Alias(key) | ImplMethodBinding::Fresh(key) => key,
            };
            let actual = index.functions.get(signature_key).with_context(|| {
                format!("E-IMPL-006: implementation method `{signature_key}` is not registered")
            })?;
            // The registered method signature may still refer to `Self` in
            // receiver, argument, or return position (e.g. an inherent
            // method bound into the impl via a fresh `$function`, or an
            // alias to one). Substitute it with the implementing type here,
            // the same way `expected` above was substituted, so conformance
            // is checked between two fully concrete signatures.
            let actual_arg_types: Vec<TypeRef> = actual
                .arg_types
                .iter()
                .map(|ty| type_semantics::substitute_self(ty, &self_type))
                .collect();
            let actual_return_type =
                type_semantics::substitute_self(&actual.return_type, &self_type);
            let expected_args = match *args {
                TypeRef::Tuple(args) => args,
                other => vec![other],
            };
            let args_match = actual_arg_types
                .iter()
                .zip(&expected_args)
                .all(|(actual, expected)| equivalent_type(actual, expected, index));
            if actual_arg_types.len() != expected_args.len()
                || !args_match
                || !equivalent_type(&actual_return_type, &return_type, index)
            {
                bail!(
                    "E-IMPL-005: signature of `{signature_key}` does not match `{}.{method}`; expected {:?} -> {:?}, got {:?} -> {:?}",
                    key.interface,
                    expected_args,
                    return_type,
                    actual_arg_types,
                    actual_return_type
                );
            }
        }
    }
    Ok(())
}

fn equivalent_type(left: &TypeRef, right: &TypeRef, index: &TypedSignatureIndex) -> bool {
    normalize_type(left, index) == normalize_type(right, index)
}

fn normalize_type(ty: &TypeRef, index: &TypedSignatureIndex) -> TypeRef {
    type_semantics::normalize_type_ref(ty, &index.aliases)
}

fn validate_bounds(
    context: &str,
    parameters: &[String],
    bounds: &[Vec<TypeRef>],
    index: &TypedSignatureIndex,
) -> Result<()> {
    if bounds.len() != parameters.len() {
        bail!(
            "E-WHERE-001: `{context}` has {} type parameters but {} bound lists",
            parameters.len(),
            bounds.len()
        );
    }
    for (parameter, list) in parameters.iter().zip(bounds) {
        for bound in list {
            if !is_interface_type(bound, index, &mut BTreeSet::new()) {
                bail!(
                    "E-WHERE-002: bound for `{parameter}` of `{context}` is not an interface: {bound:?}"
                );
            }
            validate_type(bound, parameters, bounds, context, index)?;
        }
    }
    Ok(())
}

fn is_interface_type(
    ty: &TypeRef,
    index: &TypedSignatureIndex,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match ty {
        TypeRef::Interface(_) => true,
        TypeRef::Named(name) | TypeRef::Instantiated { base: name, .. } => {
            if !visiting.insert(name.clone()) {
                return false;
            }
            let result = index
                .aliases
                .get(name)
                .is_some_and(|alias| is_interface_type(&alias.body, index, visiting));
            visiting.remove(name);
            result
        }
        TypeRef::Intersect(parts) => {
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|part| is_interface_type(part, index, visiting))
        }
        _ => false,
    }
}

fn validate_type(
    ty: &TypeRef,
    parameters: &[String],
    bounds: &[Vec<TypeRef>],
    context: &str,
    index: &TypedSignatureIndex,
) -> Result<()> {
    match ty {
        TypeRef::Named(name) => {
            if let Some(alias) = index.aliases.get(name) {
                if !alias.type_params.is_empty() {
                    bail!(
                        "E-TYPE-ARITY: `{name}` expects {} type arguments, got 0 in `{context}`",
                        alias.type_params.len()
                    );
                }
            }
        }
        TypeRef::Instantiated { base, type_args } => {
            let Some(alias) = index.aliases.get(base) else {
                bail!("E-TYPE-UNKNOWN: unknown type application `{base}` in `{context}`");
            };
            if type_args.len() != alias.type_params.len() {
                bail!(
                    "E-TYPE-ARITY: `{base}` expects {} type arguments, got {} in `{context}`",
                    alias.type_params.len(),
                    type_args.len()
                );
            }
            for (position, argument) in type_args.iter().enumerate() {
                for required in alias.type_param_bounds.get(position).into_iter().flatten() {
                    if !satisfies_bound(argument, required, parameters, bounds, index) {
                        bail!(
                            "E-BOUND-001: `{argument:?}` does not satisfy `{required:?}` for `{base}` in `{context}`"
                        );
                    }
                }
            }
            for argument in type_args {
                validate_type(argument, parameters, bounds, context, index)?;
            }
        }
        TypeRef::Mutable(inner)
        | TypeRef::Array(inner)
        | TypeRef::JoinHandle(inner)
        | TypeRef::Newtype { inner, .. }
        | TypeRef::Reference { inner, .. } => {
            validate_type(inner, parameters, bounds, context, index)?
        }
        TypeRef::Map { key, value } => {
            validate_type(key, parameters, bounds, context, index)?;
            validate_type(value, parameters, bounds, context, index)?;
        }
        TypeRef::Record(fields) | TypeRef::Enum(fields) | TypeRef::Interface(fields) => {
            for member in fields.values() {
                validate_type(member, parameters, bounds, context, index)?;
            }
        }
        TypeRef::Tuple(items) | TypeRef::Union(items) | TypeRef::Intersect(items) => {
            for item in items {
                validate_type(item, parameters, bounds, context, index)?;
            }
        }
        TypeRef::FnType { args, return_type } => {
            validate_type(args, parameters, bounds, context, index)?;
            validate_type(return_type, parameters, bounds, context, index)?;
        }
        _ => {}
    }
    Ok(())
}

fn satisfies_bound(
    argument: &TypeRef,
    required: &TypeRef,
    parameters: &[String],
    bounds: &[Vec<TypeRef>],
    index: &TypedSignatureIndex,
) -> bool {
    let required_interfaces = interface_requirements(required, index);
    match argument {
        TypeRef::Named(name) | TypeRef::Instantiated { base: name, .. } => {
            required_interfaces
                .iter()
                .all(|(interface, required_args)| {
                    index
                        .impls
                        .get(&ImplKey {
                            implementing_type: name.clone(),
                            interface: interface.clone(),
                        })
                        .is_some_and(|implementation| {
                            implementation.interface_args.len() == required_args.len()
                                && implementation.interface_args.iter().zip(required_args).all(
                                    |(actual, required)| equivalent_type(actual, required, index),
                                )
                        })
                })
        }
        TypeRef::Generic(name) => {
            let Some(position) = parameters.iter().position(|parameter| parameter == name) else {
                return false;
            };
            let provided: Vec<_> = bounds
                .get(position)
                .into_iter()
                .flatten()
                .flat_map(|bound| interface_requirements(bound, index))
                .collect();
            required_interfaces.iter().all(|required| {
                provided.iter().any(|candidate| {
                    candidate.0 == required.0
                        && candidate.1.len() == required.1.len()
                        && candidate
                            .1
                            .iter()
                            .zip(&required.1)
                            .all(|(left, right)| equivalent_type(left, right, index))
                })
            })
        }
        _ => false,
    }
}

fn interface_requirements(
    ty: &TypeRef,
    index: &TypedSignatureIndex,
) -> Vec<(String, Vec<TypeRef>)> {
    match ty {
        TypeRef::Named(name) => index
            .aliases
            .get(name)
            .map_or_else(Vec::new, |alias| match &alias.body {
                TypeRef::Interface(_) => vec![(name.clone(), Vec::new())],
                TypeRef::Intersect(parts) => parts
                    .iter()
                    .flat_map(|part| interface_requirements(part, index))
                    .collect(),
                _ => Vec::new(),
            }),
        TypeRef::Instantiated { base, type_args } => {
            index
                .aliases
                .get(base)
                .map_or_else(Vec::new, |alias| match &alias.body {
                    TypeRef::Interface(_) => vec![(base.clone(), type_args.clone())],
                    TypeRef::Intersect(parts) => parts
                        .iter()
                        .flat_map(|part| interface_requirements(part, index))
                        .collect(),
                    _ => Vec::new(),
                })
        }
        TypeRef::Intersect(parts) => parts
            .iter()
            .flat_map(|part| interface_requirements(part, index))
            .collect(),
        _ => Vec::new(),
    }
}

/// Widened to `pub(crate)` so `typed_body.rs` can resolve an `impls:`
/// annotation's interface name the same way signature lowering does, to
/// derive the exact staged body key `ImplMethodBinding::Fresh` registered.
pub(crate) fn named_application(ty: &TypeRef) -> Result<(String, Vec<TypeRef>)> {
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

pub(crate) fn qualify(module_alias: &str, name: &str) -> String {
    if module_alias.is_empty() {
        name.to_string()
    } else {
        format!("{module_alias}.{name}")
    }
}

/// Widened to `pub(crate)` for the same reason as `named_application`: it is
/// half of the `owner` derivation `typed_body.rs` must reproduce exactly.
pub(crate) fn unqualify<'a>(module_alias: &str, name: &'a str) -> &'a str {
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
(def option (union t void) where: ((t)))
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
            r#"(def comparable (interface))
(def display (interface (show (fn-type (self) str))) where: ((t)))
(fn show-box ((value (box t))) str (do (return "box")) where: ((t comparable)))
(def box (record (value t))
  where: ((t comparable))
  doc: "A box."
  defs: ((fn get ((input self)) t (do (return unit))))
  impls: ((impl display
    types: (t)
    methods: ((method show show-box)))))
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
            interface: "pkg.display".into(),
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

    #[test]
    fn lowers_domain_reference_and_wasm_types_without_yaml_adapters() {
        let source = module(
            r#"(fn host-types
  ((input (handle read))
   (shared (ref str))
   (exclusive (mut-ref int64))
   (raw (wasm i32)))
  int32
  (do (return raw)))"#,
            6,
        );
        let index = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &source,
        }])
        .unwrap();
        let signature = &index.functions["host-types"];
        assert_eq!(
            signature.arg_types[0],
            TypeRef::HostHandle(HandleAccess::Read)
        );
        assert_eq!(
            signature.arg_types[1],
            TypeRef::Reference {
                inner: Box::new(TypeRef::Str),
                mutable: false,
            }
        );
        assert_eq!(
            signature.arg_types[2],
            TypeRef::Reference {
                inner: Box::new(TypeRef::Int64),
                mutable: true,
            }
        );
        assert_eq!(signature.arg_types[3], TypeRef::Int32);
        assert_eq!(signature.return_type, TypeRef::Int32);
    }

    #[test]
    fn validates_alias_arity_bounds_and_complete_interfaces() {
        let wrong_arity = module(
            r#"(def pair (record (left t) (right t)) where: ((t)))
(fn bad ((value pair)) void (do (return unit)))"#,
            7,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &wrong_arity,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-TYPE-ARITY"));

        let unsatisfied_bound = module(
            r#"(def display (interface (show (fn-type (self) str))))
(def printable-box (record (value t)) where: ((t display)))
(def plain (record (value str)))
(fn bad ((value (printable-box plain))) void (do (return unit)))"#,
            8,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &unsatisfied_bound,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-BOUND-001"));

        let incomplete_impl = module(
            r#"(def display (interface
  (show (fn-type (self) str))
  (debug (fn-type (self) str))))
(fn show-item ((value item)) str (do (return "item")))
(def item (record (value str))
  impls: ((impl display
    methods: ((method show show-item)))))"#,
            9,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &incomplete_impl,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-IMPL-003"));
    }

    #[test]
    fn rejects_unknown_applications_bad_impl_signatures_and_nested_mutability() {
        let unknown = module(
            "(fn bad ((value (missing int64))) void (do (return unit)))",
            10,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &unknown,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-TYPE-UNKNOWN"));

        let mismatch = module(
            r#"(def display (interface (show (fn-type (self) str))))
(fn wrong ((value int64)) int64 (do (return value)))
(def item (record (value str))
  impls: ((impl display methods: ((method show wrong)))))"#,
            11,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &mismatch,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-IMPL-005"));

        let fresh_mismatch = module(
            r#"(def display (interface (show (fn-type (self) str))))
(def item (record (value str))
  impls: ((impl display
    methods: ((method show
      (fn wrong ((value int64)) int64 (do (return value))))))))"#,
            16,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &fresh_mismatch,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-IMPL-005"));

        let nested = module(
            "(fn bad ((value (mut-ref (ref int64)))) void (do (return unit)))",
            12,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &nested,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("cannot wrap"));

        let hidden_nested = module(
            "(fn bad ((value (ref (mut (ref int64))))) void (do (return unit)))",
            20,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &hidden_nested,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("cannot contain"));

        let normalized = module(
            "(fn borrow ((value (ref (mut int64)))) void (do (return unit)))",
            13,
        );
        let index = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &normalized,
        }])
        .unwrap();
        assert_eq!(
            index.functions["borrow"].arg_types,
            [TypeRef::Reference {
                inner: Box::new(TypeRef::Int64),
                mutable: true,
            }]
        );
    }

    #[test]
    fn substitutes_self_before_checking_impl_conformance() {
        // Self in receiver position: the impl method's own signature names
        // its receiver as `self`, while the interface it implements already
        // names the concrete implementing type. Conformance must substitute
        // `Self` on the impl side before comparing.
        let receiver_self = module(
            r#"(def closeable (interface (close (fn-type (self) str))))
(def thing (record (value str))
  impls: ((impl closeable
    methods: ((method close
      (fn close-thing ((value self)) str (do (return "closed"))))))))"#,
            21,
        );
        lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &receiver_self,
        }])
        .unwrap();

        // Self in receiver position plus an additional, non-Self argument.
        let receiver_self_plus_arg = module(
            r#"(def appendable (interface (append-bytes (fn-type (self int64) str))))
(def thing2 (record (value str))
  impls: ((impl appendable
    methods: ((method append-bytes
      (fn append-thing ((value self) (extra int64)) str (do (return "appended"))))))))"#,
            22,
        );
        lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &receiver_self_plus_arg,
        }])
        .unwrap();

        // Self in return position.
        let return_self = module(
            r#"(def cloneable (interface (clone (fn-type (self) self))))
(def thing3 (record (value str))
  impls: ((impl cloneable
    methods: ((method clone
      (fn clone-thing ((value self)) self (do (return value))))))))"#,
            23,
        );
        lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &return_self,
        }])
        .unwrap();

        // A genuine signature mismatch (wrong return type) must still be
        // rejected: substitution must not weaken the conformance check.
        let genuine_mismatch = module(
            r#"(def closeable2 (interface (close (fn-type (self) str))))
(def bad-thing (record (value str))
  impls: ((impl closeable2
    methods: ((method close
      (fn close-bad ((value self)) int64 (do (return 0))))))))"#,
            24,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &genuine_mismatch,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-IMPL-005"));
    }

    #[test]
    fn enforces_interface_argument_bounds_and_referenced_method_scope() {
        let bound = module(
            r#"(def display (interface (show (fn-type (self) str))))
(def factory (interface) where: ((t display)))
(def plain (record (value str)))
(def maker (record)
  impls: ((impl factory types: (plain) methods: ())))"#,
            14,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &bound,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-BOUND-001"));

        let parameterized_bound = module(
            r#"(def relates (interface) where: ((u)))
(def displayable (interface) where: ((t (relates int64))))
(def plain (record)
  impls: ((impl relates types: (bool) methods: ())))
(def holder (record)
  impls: ((impl displayable types: (plain) methods: ())))"#,
            17,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &parameterized_bound,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-BOUND-001"));

        let unresolved = module(
            r#"(def display (interface (show (fn-type (self) str))))
(def item (record)
  impls: ((impl display methods: ((method show missing)))))"#,
            15,
        );
        let error = lower_typed_signatures([TypedModuleInput {
            alias: "",
            module: &unresolved,
        }])
        .unwrap_err();
        assert!(error.to_string().contains("E-IMPL-006"));

        let library = module(
            r#"(private (def item (record)
  defs: ((fn show ((value self)) str (do (return "private"))))))"#,
            18,
        );
        let consumer = module(
            r#"(import lib "./lib.vibra")
(def display (interface (show (fn-type (self) str))))
(def item (record)
  impls: ((impl display methods: ((method show lib.item.show)))))"#,
            19,
        );
        let error = lower_typed_signatures([
            TypedModuleInput {
                alias: "lib",
                module: &library,
            },
            TypedModuleInput {
                alias: "",
                module: &consumer,
            },
        ])
        .unwrap_err();
        assert!(error.to_string().contains("not publicly visible"));
    }
}
