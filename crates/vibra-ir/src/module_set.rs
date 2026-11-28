//! A checked module set: every declaration of every module a program is
//! built from, validated as a whole.
//!
//! [`CheckedModuleSet::try_new`] checks every type, every expression's
//! shape, every implementation against its interface, that no two
//! implementations overlap, and the rules of the effects chapter that a set
//! keeps (`crate::effects`), and records the static dependency edges of each
//! initializer, function, and closure body. A module set is immutable and
//! shared; a checker defect that reaches it is an [`IrError`], never an
//! execution.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::conformance::Evidence;
use crate::constant::check_encodable;
use crate::effects;
use crate::encode::{TypeTable, encode_value};
use crate::expr::{Checker, Tables, expect};
use crate::external::CompilerIntrinsic;
use crate::external::host::{HostOp, TestWorldOp};
use crate::instantiation::{check_built_dicts, check_instantiations};
use crate::{
    Constant, ExitGuard, FnType, Guard, Implementation, InterfaceDefinition, IrError,
    Library, LibraryTypes, Node, OpenScope, Row, SignatureRecord, SourceOrigin,
    Substitution, Type, TypeBody, TypeDefinition, TypeId,
};

/// A module value: an immutable, typed, lazily initialized `let`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedGlobal {
    /// The value's qualified name, for diagnostics.
    pub name: String,
    /// Its closed type.
    pub value_type: Type,
    /// The initializer.
    pub initializer: Node,
    /// The number of slots the initializer's activation has.
    pub slot_count: usize,
    /// Where it was declared.
    pub origin: SourceOrigin,
}

/// What a function runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionBody {
    /// A checked body.
    Expr(Box<Node>),
    /// A row of the `@compiler` registry, bound with `external @compiler`,
    /// at type arguments over the function's binders.
    Intrinsic {
        /// The row.
        intrinsic: CompilerIntrinsic,
        /// One type for each of the row's generics.
        type_arguments: Vec<Type>,
    },
    /// A member bound with `external @host`: the implementation of a
    /// sealed grant of the interface that declares the member.
    Host(HostBinding),
}

/// The registry row an `external @host` member is bound to
/// (`docs/spec/06-runtime.md`, "Host registry").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostBinding {
    /// One of the 17 rows of the host registry.
    Operation(HostOp),
    /// One of the six rows of the test-world registry.
    TestWorld(TestWorldOp),
}

impl HostBinding {
    /// The row's wire symbol, such as `fs.readText`.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Operation(op) => op.symbol(),
            Self::TestWorld(op) => op.symbol(),
        }
    }
}

/// Whether a function's body runs in a procedure context
/// (`docs/spec/03-effects.md`, "Cold plans and performed rows"). A function
/// of a `Proc` type runs in one by its type; `Procedure` gives one to a
/// function of a `Fn` type, a test body, with the row it states.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BodyContext {
    /// No context beyond the function's type: a `fn` performs nothing.
    #[default]
    Pure,
    /// A procedure context whose ceiling is this row.
    Procedure(Row),
}

/// A module function: a `fn` or a `proc`, a member of an implementation, or
/// the default body of an interface member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFunction {
    /// The function's qualified name, for diagnostics.
    pub name: String,
    /// Its signature record. A default body of an interface member has the
    /// member's record.
    pub signature: SignatureRecord,
    /// The constant default of each labelled parameter that has one.
    pub defaults: Vec<(String, Constant)>,
    /// What it runs.
    pub body: FunctionBody,
    /// `requires C else V`, when the function has one.
    pub entry_guard: Option<Guard>,
    /// `ensures C else V`, when the function has one.
    pub exit_guard: Option<ExitGuard>,
    /// The number of slots its activation has: its argument slots first.
    pub slot_count: usize,
    /// Where it was declared.
    pub origin: SourceOrigin,
    /// Whether its body runs in a procedure context: a `proc`, or a test.
    pub context: BodyContext,
}

impl CheckedFunction {
    /// A function whose body is the `@compiler` row `intrinsic` at
    /// `type_arguments`.
    #[must_use]
    pub fn external(
        name: impl Into<String>,
        signature: SignatureRecord,
        intrinsic: CompilerIntrinsic,
        type_arguments: Vec<Type>,
        origin: SourceOrigin,
    ) -> Self {
        let slot_count = match signature.type_info() {
            Type::Fn(callable) => callable.slot_count(),
            _ => 0,
        };
        Self {
            name: name.into(),
            signature,
            defaults: Vec::new(),
            body: FunctionBody::Intrinsic {
                intrinsic,
                type_arguments,
            },
            entry_guard: None,
            exit_guard: None,
            slot_count,
            origin,
            context: BodyContext::Pure,
        }
    }
}

/// Everything a module set is built from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleSetParts {
    /// The identity bound to each library declaration.
    pub library: LibraryTypes,
    /// Every declared type.
    pub types: Vec<TypeDefinition>,
    /// Every interface.
    pub interfaces: Vec<InterfaceDefinition>,
    /// Every module value, by index.
    pub globals: Vec<CheckedGlobal>,
    /// Every function, by index.
    pub functions: Vec<CheckedFunction>,
    /// Every implementation.
    pub implementations: Vec<Implementation>,
}

/// A callable that may escape as a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Callable {
    /// A module function.
    Function(usize),
    /// A closure, numbered in the order the module set's bodies create them.
    Closure(usize),
}

/// The static facts of one body: what it reads, enters, and lets escape.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// The module values it reads.
    pub reads: BTreeSet<usize>,
    /// The functions it enters directly: direct calls, every implementation
    /// and default a contract call may select, the invariant predicates of
    /// its constructions, and the `Ordered.compare` members a dict uses.
    pub calls: BTreeSet<usize>,
    /// The callables it creates as values, each with its type.
    pub values: Vec<(Callable, Type)>,
    /// The callee type of each call of a value: a call through a callable
    /// value, or a row that applies its function operand.
    pub indirect: Vec<Type>,
    /// The row of each plan it awaits, in the order the awaits are checked.
    pub awaited: Vec<Row>,
}

impl Facts {
    fn rebase(&mut self, by: usize) {
        for (callable, _) in &mut self.values {
            if let Callable::Closure(index) = callable {
                *index = index.saturating_add(by);
            }
        }
    }
}

/// The static facts of a whole module set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flow {
    /// One for each module value's initializer.
    pub globals: Vec<Facts>,
    /// One for each function: its body and its guards.
    pub functions: Vec<Facts>,
    /// One for each closure: its type and its body's facts.
    pub closures: Vec<(Type, Facts)>,
}

/// A validated module set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedModuleSet {
    table: TypeTable,
    interfaces: BTreeMap<TypeId, InterfaceDefinition>,
    globals: Vec<CheckedGlobal>,
    functions: Vec<CheckedFunction>,
    implementations: Vec<Implementation>,
    flow: Flow,
}

/// The value bound to each variable of a unification, resolved through the
/// others; an unbound one stays a binder reference.
pub(crate) fn resolved(assignment: &[Option<Type>]) -> Vec<Type> {
    let mut resolved = assignment
        .iter()
        .enumerate()
        .map(|(index, bound)| bound.clone().unwrap_or(Type::Binder(index)))
        .collect::<Vec<_>>();
    // The occurs check keeps the bindings acyclic, so this many rounds
    // replace every bound variable.
    for _ in 0..assignment.len() {
        resolved = resolved
            .iter()
            .map(|ty| ty.instantiate(&resolved))
            .collect();
    }
    resolved
}

/// `ty`, with a bound variable at its root replaced by its binding.
fn walk(ty: &Type, assignment: &[Option<Type>]) -> Type {
    let mut current = ty.clone();
    for _ in 0..=assignment.len() {
        match &current {
            Type::Binder(index) => {
                match assignment.get(*index).and_then(Option::as_ref) {
                    Some(bound) => current = bound.clone(),
                    None => break,
                }
            }
            _ => break,
        }
    }
    current
}

fn is_open_form(ty: &Type) -> bool {
    matches!(ty, Type::Binder(_) | Type::SelfType | Type::Associated(_))
}

/// Whether two types have the same outermost form: the same case, and the
/// same identity, names, atom, or row where the case has one.
pub(crate) fn same_head(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (Type::AtomSingleton(left), Type::AtomSingleton(right))
        | (Type::Associated(left), Type::Associated(right)) => left == right,
        (Type::List(_), Type::List(_)) => true,
        (Type::Tuple(left), Type::Tuple(right)) => left.len() == right.len(),
        (Type::Record(left), Type::Record(right)) => left.names().eq(right.names()),
        (Type::Fn(left), Type::Fn(right)) => {
            left.parameters.len() == right.parameters.len()
                && left.labels.names().eq(right.labels.names())
        }
        (Type::Proc(left), Type::Proc(right)) => {
            left.parameters.len() == right.parameters.len()
                && left.labels.names().eq(right.labels.names())
                && left.effects == right.effects
        }
        (Type::Intent(left), Type::Intent(right)) => left.effects == right.effects,
        (
            Type::Declared(left, left_arguments),
            Type::Declared(right, right_arguments),
        ) => left == right && left_arguments.len() == right_arguments.len(),
        (
            Type::Interface(left, left_arguments, left_choices),
            Type::Interface(right, right_arguments, right_choices),
        ) => {
            left == right
                && left_arguments.len() == right_arguments.len()
                && left_choices.names().eq(right_choices.names())
        }
        (left, right) => left.components().is_empty() && left == right,
    }
}

fn occurs(index: usize, ty: &Type, assignment: &[Option<Type>]) -> bool {
    let ty = ty.instantiate(&resolved(assignment));
    let mut found = false;
    ty.visit(&mut |inner| found |= *inner == Type::Binder(index));
    found
}

/// Unifies `left` and `right`, whose binder references are the variables of
/// `assignment`, extending it.
pub(crate) fn unify(
    left: &Type,
    right: &Type,
    assignment: &mut Vec<Option<Type>>,
) -> bool {
    let left = walk(left, assignment);
    let right = walk(right, assignment);
    match (&left, &right) {
        (Type::Binder(left), Type::Binder(right)) if left == right => true,
        (Type::Binder(index), other) | (other, Type::Binder(index))
            if *index < assignment.len() =>
        {
            if occurs(*index, other, assignment) {
                return false;
            }
            if let Some(slot) = assignment.get_mut(*index) {
                *slot = Some(other.clone());
            }
            true
        }
        _ => {
            same_head(&left, &right)
                && left
                    .components()
                    .into_iter()
                    .zip(right.components())
                    .all(|(left, right)| unify(left, right, assignment))
        }
    }
}

/// Whether a value of `created` may be called at `callee`: open forms match
/// anything, and otherwise the forms agree throughout.
pub(crate) fn may_match(created: &Type, callee: &Type) -> bool {
    if is_open_form(created) || is_open_form(callee) {
        return true;
    }
    same_head(created, callee)
        && created
            .components()
            .into_iter()
            .zip(callee.components())
            .all(|(left, right)| may_match(left, right))
}

/// The facts a body's check gathers, kept beside the checker: the body's
/// own, and each closure it creates at any depth, numbered from zero in
/// the order their bodies finish.
#[derive(Debug, Default)]
pub(crate) struct Gathered {
    pub(crate) facts: Facts,
    pub(crate) closures: Vec<(Type, Facts)>,
}

/// The functions a contract call of `member` on `interface` may select.
pub(crate) fn contract_targets(
    tables: &Tables<'_>,
    interface: &TypeId,
    member: &str,
) -> BTreeSet<usize> {
    let mut targets = tables
        .implementations
        .iter()
        .filter(|implementation| implementation.contract().interface == *interface)
        .filter_map(|implementation| implementation.member(member))
        .collect::<BTreeSet<_>>();
    if let Some(default) = tables
        .interfaces
        .get(interface)
        .and_then(|definition| definition.member(member))
        .and_then(|declared| declared.default)
    {
        targets.insert(default);
    }
    targets
}

pub(crate) fn ordered_targets(tables: &Tables<'_>) -> BTreeSet<usize> {
    tables
        .library
        .get(Library::Ordered)
        .map(|ordered| contract_targets(tables, ordered, "compare"))
        .unwrap_or_default()
}

fn check_names_distinct<'a>(
    what: &'static str,
    names: impl IntoIterator<Item = &'a TypeId>,
) -> Result<(), IrError> {
    let mut seen = BTreeSet::new();
    for id in names {
        if !seen.insert(id) {
            return Err(IrError::Duplicate {
                what,
                name: id.path().to_owned(),
            });
        }
    }
    Ok(())
}

/// The case names and field lists a library variant type must have.
fn library_cases(
    library: Library,
) -> Option<&'static [(&'static str, &'static [&'static str])]> {
    Some(match library {
        Library::Bool => &[("false", &[]), ("true", &[])],
        Library::Option => &[("none", &[]), ("some", &["value"])],
        Library::Result => &[("ok", &["value"]), ("err", &["error"])],
        Library::Ordering => &[("less", &[]), ("equal", &[]), ("greater", &[])],
        Library::ArithmeticError => &[
            ("overflow", &[]),
            ("divisionByZero", &[]),
            ("invalidShift", &[]),
        ],
        Library::ConversionError => &[
            ("outOfRange", &[]),
            ("invalidFormat", &[]),
            ("unrepresentable", &[]),
        ],
        _ => return None,
    })
}

fn check_library(
    library: &LibraryTypes,
    types: &BTreeMap<TypeId, TypeDefinition>,
    interfaces: &BTreeMap<TypeId, InterfaceDefinition>,
) -> Result<(), IrError> {
    for (declaration, id) in library.bindings() {
        let wrong = || {
            IrError::InvalidType(format!(
                "`{}` does not have the shape of `{}`",
                id.path(),
                declaration.name()
            ))
        };
        if declaration.is_interface() {
            if !interfaces.contains_key(id) {
                return Err(IrError::UnknownInterface(id.path().to_owned()));
            }
            continue;
        }
        let definition = types
            .get(id)
            .ok_or_else(|| IrError::UnknownType(id.path().to_owned()))?;
        if definition.parameters().len() != declaration.arity() {
            return Err(wrong());
        }
        let fits = match (declaration, definition.body()) {
            (Library::Str, TypeBody::Wrapper(Type::List(element))) => {
                **element == Type::Char
            }
            (Library::Bytes, TypeBody::Wrapper(Type::List(element))) => {
                **element == Type::U8
            }
            (Library::Dict, _) => true,
            (_, TypeBody::Variant(cases)) => {
                library_cases(declaration).is_some_and(|expected| {
                    cases.len() == expected.len()
                        && cases.iter().zip(expected).all(|(case, (name, fields))| {
                            case.name == *name
                                && case.fields.len() == fields.len()
                                && case
                                    .fields
                                    .iter()
                                    .zip(*fields)
                                    .all(|((field, _), expected)| field == expected)
                        })
                })
            }
            _ => false,
        };
        if !fits {
            return Err(wrong());
        }
    }
    // A narrowing type is an ordinary declaration of the set, with no
    // parameters: the grant rows name it unapplied.
    for (declaration, id) in library.narrowing_bindings() {
        let definition = types
            .get(id)
            .ok_or_else(|| IrError::UnknownType(id.path().to_owned()))?;
        if !definition.parameters().is_empty() {
            return Err(IrError::InvalidType(format!(
                "`{}` does not have the shape of `{}`",
                id.path(),
                declaration.name()
            )));
        }
    }
    Ok(())
}

/// The type of an implementation member: the interface member's type under
/// the implementation's substitution, with the member's own binders after
/// the implementation's.
fn implemented_type(
    member: &SignatureRecord,
    implementation: &Implementation,
    interface_parameters: usize,
) -> Type {
    let own = member.binders().len().saturating_sub(interface_parameters);
    let base = implementation.binders().len();
    let binders = implementation
        .contract()
        .arguments
        .iter()
        .cloned()
        .chain((0..own).map(|index| Type::Binder(base.saturating_add(index))))
        .collect::<Vec<_>>();
    member.type_info().substitute(&Substitution {
        binders: &binders,
        self_type: Some(implementation.receiver()),
        associated: Some(implementation.choices()),
    })
}

impl CheckedModuleSet {
    /// Validates `parts` as one module set.
    ///
    /// # Errors
    ///
    /// The first defect found: a duplicate identity; a type, signature, or
    /// constant that is malformed or names nothing; a library binding of the
    /// wrong shape; an implementation that does not fit its interface or
    /// overlaps another; an expression whose slots, arity, or types
    /// disagree; or a rule of the effects chapter that a body, a row, or a
    /// host binding breaks.
    #[allow(clippy::too_many_lines)]
    pub fn try_new(parts: ModuleSetParts) -> Result<Arc<Self>, IrError> {
        let ModuleSetParts {
            library,
            types,
            interfaces,
            globals,
            functions,
            implementations,
        } = parts;
        check_names_distinct("type", types.iter().map(TypeDefinition::id))?;
        check_names_distinct(
            "interface",
            interfaces.iter().map(InterfaceDefinition::id),
        )?;
        let table = TypeTable::new(library, types);
        let interfaces = interfaces
            .into_iter()
            .map(|definition| (definition.id().clone(), definition))
            .collect::<BTreeMap<_, _>>();
        check_library(table.library(), table.types(), &interfaces)?;
        let set = Self {
            table,
            interfaces,
            globals,
            functions,
            implementations,
            flow: Flow::default(),
        };
        let flow = set.validate()?;
        let set = Self { flow, ..set };
        effects::validate(&set)?;
        Ok(Arc::new(set))
    }

    /// Whether a value of `held`, a closed type, may stand behind an
    /// interface value of `interface_type`: `held` is valid, is no interface
    /// value type, and widens to `interface_type` as a checked
    /// `WidenToInterface` would.
    ///
    /// # Errors
    ///
    /// `held` is malformed or names nothing, or the evidence that it
    /// implements the interface needs more goals than validation allows.
    pub fn widens(&self, held: &Type, interface_type: &Type) -> Result<bool, IrError> {
        let tables = self.tables();
        tables.check_type(held, OpenScope::Closed)?;
        if held.is_interface_value() {
            return Ok(false);
        }
        match interface_type {
            Type::Any => Ok(true),
            Type::Interface(interface, arguments, choices) => tables.conforms_with(
                Evidence::default(),
                held,
                interface,
                arguments,
                choices,
            ),
            _ => Ok(false),
        }
    }

    /// The type of the function at `index` as a value at `type_arguments`,
    /// which are closed and satisfy its bounds.
    ///
    /// # Errors
    ///
    /// No function `index`, a type argument that is malformed, open, or of
    /// the wrong count, or a bound that does not hold.
    pub fn function_value_type(
        &self,
        index: usize,
        type_arguments: &[Type],
    ) -> Result<Type, IrError> {
        let tables = self.tables();
        for argument in type_arguments {
            tables.check_type(argument, OpenScope::Closed)?;
        }
        let ty = tables.function_type(index, type_arguments)?;
        tables.check_bounds(
            Evidence::default(),
            tables.function(index)?.signature.binders(),
            &Substitution {
                binders: type_arguments,
                ..Substitution::default()
            },
        )?;
        Ok(ty)
    }

    fn tables(&self) -> Tables<'_> {
        Tables {
            library: self.table.library(),
            types: self.table.types(),
            table: &self.table,
            interfaces: &self.interfaces,
            implementations: &self.implementations,
            globals: &self.globals,
            functions: &self.functions,
        }
    }

    fn check_constant(&self, constant: &Constant) -> Result<(), IrError> {
        self.tables()
            .check_type(constant.value_type(), OpenScope::Closed)?;
        check_encodable("a constant", constant.value_type(), &self.interfaces)?;
        encode_value(constant.value(), constant.value_type(), &self.table)
            .map(drop)
            .map_err(|error| IrError::InvalidExpression(format!("a constant: {error}")))
    }

    fn validate(&self) -> Result<Flow, IrError> {
        let tables = self.tables();
        let bool_type = || table_bool(&self.table);
        for definition in self.table.types().values() {
            let scope = OpenScope::Binders(definition.parameters().len());
            let components: Vec<&Type> = match definition.body() {
                TypeBody::Record(fields) => fields.iter().map(|(_, ty)| ty).collect(),
                TypeBody::Tuple(values) => values.iter().collect(),
                TypeBody::Wrapper(value) => vec![value],
                TypeBody::Variant(cases) => cases
                    .iter()
                    .flat_map(|case| case.fields.iter().map(|(_, ty)| ty))
                    .collect(),
            };
            let evidence = Evidence {
                binders: definition.parameters(),
                self_interface: None,
            };
            for component in components {
                tables.check_type(component, scope)?;
                tables.check_applications(evidence, component)?;
            }
            if let Some(invariant) = definition.invariant() {
                let function = tables.function(invariant.predicate)?;
                let representation =
                    definition.representation_type().ok_or_else(|| {
                        IrError::InvalidType(format!(
                            "`{}` has no representation",
                            definition.id().path()
                        ))
                    })?;
                let expected =
                    Type::function(FnType::new(vec![representation], bool_type()?));
                if function.signature.binders().len() != definition.parameters().len()
                    || *function.signature.type_info() != expected
                {
                    return Err(IrError::InvalidType(format!(
                        "the predicate of `{}` is not a Fn({{R}} Bool) over its parameters",
                        definition.id().path()
                    )));
                }
                if let Some(consequence) = &invariant.consequence {
                    self.check_constant(consequence)?;
                }
            }
        }
        check_finite_size(&self.table)?;
        let mut defaults = BTreeMap::new();
        for interface in self.interfaces.values() {
            for member in interface.members() {
                tables.check_type(
                    member.signature.type_info(),
                    member.signature.scope(),
                )?;
                tables.check_applications(
                    Evidence {
                        binders: member.signature.binders(),
                        self_interface: Some(interface.id()),
                    },
                    member.signature.type_info(),
                )?;
                if !self_map_result(member.signature.type_info(), self.table.library())
                {
                    return Err(IrError::InvalidType(format!(
                        "the result of `{}.{}` holds `Self` outside the positions an interface value reboxes",
                        interface.id().path(),
                        member.name
                    )));
                }
                if let Some(default) = member.default {
                    defaults.insert(default, interface.id());
                }
                if let Some(default) = member.default
                    && tables.function(default)?.signature != member.signature
                {
                    return Err(IrError::InvalidType(format!(
                        "the default of `{}.{}` does not have the member's signature",
                        interface.id().path(),
                        member.name
                    )));
                }
            }
        }
        self.validate_implementations(&tables)?;
        let mut flow = Flow::default();
        for global in &self.globals {
            tables.check_type(&global.value_type, OpenScope::Closed)?;
            tables.check_applications(Evidence::default(), &global.value_type)?;
            let mut checker =
                Checker::new(tables, OpenScope::Closed, global.slot_count);
            let mut gathered = Gathered::default();
            expect(
                &global.value_type,
                &checker
                    .check_gathering(&global.initializer, &mut gathered)
                    .map_err(|error| error.owned_by(&global.name))?,
            )?;
            flow.absorb_owner(gathered, |flow, facts| flow.globals.push(facts));
        }
        for (index, function) in self.functions.iter().enumerate() {
            let gathered = self
                .validate_function(&tables, function, defaults.get(&index).copied())
                .map_err(|error| error.owned_by(&function.name))?;
            flow.absorb_owner(gathered, |flow, facts| flow.functions.push(facts));
        }
        check_instantiations(&tables)?;
        let initializers = self
            .globals
            .iter()
            .map(|global| &global.initializer)
            .collect::<Vec<_>>();
        check_built_dicts(&tables, &initializers)?;
        Ok(flow)
    }

    fn validate_implementations(&self, tables: &Tables<'_>) -> Result<(), IrError> {
        for implementation in &self.implementations {
            let scope = implementation.record().scope();
            let contract = implementation.contract();
            let interface =
                self.interfaces.get(&contract.interface).ok_or_else(|| {
                    IrError::UnknownInterface(contract.interface.path().to_owned())
                })?;
            let path = contract.interface.path();
            if interface.parameters().len() != contract.arguments.len() {
                return Err(IrError::ArityMismatch {
                    what: "interface arguments",
                    expected: interface.parameters().len(),
                    actual: contract.arguments.len(),
                });
            }
            for ty in std::iter::once(implementation.receiver())
                .chain(&contract.arguments)
                .chain(implementation.choices().entries().iter().map(|(_, ty)| ty))
            {
                tables.check_type(ty, scope)?;
                tables.check_applications(
                    Evidence {
                        binders: implementation.binders(),
                        self_interface: None,
                    },
                    ty,
                )?;
            }
            if interface.associated().len() != implementation.choices().len()
                || !interface
                    .associated()
                    .iter()
                    .all(|name| implementation.choices().get(name).is_some())
            {
                return Err(IrError::InvalidType(format!(
                    "an implementation of `{path}` fixes every associated type"
                )));
            }
            for member in interface.members() {
                if (member.default.is_none() || host_bound(tables, member.default))
                    && implementation.member(&member.name).is_none()
                {
                    return Err(IrError::InvalidType(format!(
                        "an implementation of `{path}` does not supply `{}`",
                        member.name
                    )));
                }
            }
            for (name, function) in implementation.members() {
                let member = interface.member(name).ok_or_else(|| {
                    IrError::InvalidType(format!("`{path}` has no member `{name}`"))
                })?;
                if member.default.is_some() && !host_bound(tables, member.default) {
                    return Err(IrError::InvalidType(format!(
                        "an implementation of `{path}` supplies the default member `{name}` again"
                    )));
                }
                let expected = implemented_type(
                    &member.signature,
                    implementation,
                    interface.parameters().len(),
                );
                let function = tables.function(*function)?;
                if *function.signature.type_info() != expected {
                    return Err(IrError::TypeMismatch {
                        expected: Box::new(expected),
                        actual: Box::new(function.signature.type_info().clone()),
                    });
                }
            }
        }
        for (index, left) in self.implementations.iter().enumerate() {
            for right in self.implementations.iter().skip(index.saturating_add(1)) {
                if left.contract().interface == right.contract().interface
                    && overlaps(left, right)
                {
                    return Err(IrError::Duplicate {
                        what: "implementation",
                        name: format!(
                            "{} for {}",
                            left.contract().interface.path(),
                            left.receiver()
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_function(
        &self,
        tables: &Tables<'_>,
        function: &CheckedFunction,
        self_interface: Option<&TypeId>,
    ) -> Result<Gathered, IrError> {
        let signature = &function.signature;
        let scope = signature.scope();
        tables.check_type(signature.type_info(), scope)?;
        tables.check_applications(
            Evidence {
                binders: signature.binders(),
                self_interface,
            },
            signature.type_info(),
        )?;
        let (slot_types, labels, result) = match signature.type_info() {
            Type::Fn(callable) => (
                callable.slot_types(),
                &callable.labels,
                callable.result.clone(),
            ),
            Type::Proc(callable) => (
                callable
                    .parameters
                    .iter()
                    .cloned()
                    .chain(callable.labels.entries().iter().map(|(_, ty)| ty.clone()))
                    .collect(),
                &callable.labels,
                callable.result.clone(),
            ),
            other => {
                return Err(IrError::InvalidType(format!(
                    "the function `{}` has the type {other}, which is not callable",
                    function.name
                )));
            }
        };
        if function.slot_count < slot_types.len() {
            return Err(IrError::InvalidSlotCount {
                needed: slot_types.len(),
                declared: function.slot_count,
            });
        }
        let mut checker = Checker::new(*tables, scope, function.slot_count);
        checker.check_defaults(labels, &function.defaults)?;
        checker.evidence = Evidence {
            binders: signature.binders(),
            self_interface,
        };
        checker.result = Some(result.clone());
        for (slot, ty) in slot_types.into_iter().enumerate() {
            checker.bind(slot, ty)?;
        }
        let mut gathered = Gathered::default();
        match &function.body {
            FunctionBody::Expr(body) => {
                expect(&result, &checker.check_gathering(body, &mut gathered)?)?;
            }
            FunctionBody::Intrinsic {
                intrinsic,
                type_arguments,
            } => {
                let row = intrinsic.row();
                if row.generics.len() != type_arguments.len() {
                    return Err(IrError::ArityMismatch {
                        what: "type arguments",
                        expected: row.generics.len(),
                        actual: type_arguments.len(),
                    });
                }
                for argument in type_arguments {
                    argument.check_depth()?;
                }
                checker.check_built_dict(*intrinsic, type_arguments)?;
                if *intrinsic == CompilerIntrinsic::DictFromList {
                    gathered.facts.calls.extend(ordered_targets(tables));
                }
                let parameters = row
                    .parameters
                    .iter()
                    .map(|parameter| {
                        parameter.instantiate(tables.library, type_arguments)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let expected = Type::function(FnType::new(
                    parameters,
                    row.result.instantiate(tables.library, type_arguments)?,
                ));
                if *signature.type_info() != expected {
                    return Err(IrError::TypeMismatch {
                        expected: Box::new(expected),
                        actual: Box::new(signature.type_info().clone()),
                    });
                }
            }
            // The member, the signature, and the row of a host binding are
            // the registry row's, as `effects::validate` checks once the
            // whole set is known.
            FunctionBody::Host(_) => {}
        }
        checker.in_guard = true;
        if let Some(guard) = &function.entry_guard {
            self.check_guard(&mut checker, guard, &result, &mut gathered)?;
        }
        if let Some(exit) = &function.exit_guard {
            let held = checker.bind(exit.candidate, result.clone())?;
            let checked =
                self.check_guard(&mut checker, &exit.guard, &result, &mut gathered);
            checker.restore(vec![(exit.candidate, held)]);
            checked?;
        }
        Ok(gathered)
    }

    fn check_guard(
        &self,
        checker: &mut Checker<'_>,
        guard: &Guard,
        result: &Type,
        gathered: &mut Gathered,
    ) -> Result<(), IrError> {
        expect(
            &table_bool(&self.table)?,
            &checker.check_gathering(&guard.predicate, gathered)?,
        )?;
        expect(
            result,
            &checker.check_gathering(&guard.consequence, gathered)?,
        )
    }

    /// The declared types and the library bindings, for encoding.
    #[must_use]
    pub const fn types(&self) -> &TypeTable {
        &self.table
    }

    /// The library bindings.
    #[must_use]
    pub const fn library(&self) -> &LibraryTypes {
        self.table.library()
    }

    /// The interfaces, by identity.
    #[must_use]
    pub const fn interfaces(&self) -> &BTreeMap<TypeId, InterfaceDefinition> {
        &self.interfaces
    }

    /// The module values, by index.
    #[must_use]
    pub fn globals(&self) -> &[CheckedGlobal] {
        &self.globals
    }

    /// The functions, by index.
    #[must_use]
    pub fn functions(&self) -> &[CheckedFunction] {
        &self.functions
    }

    /// The implementations.
    #[must_use]
    pub fn implementations(&self) -> &[Implementation] {
        &self.implementations
    }

    /// The static dependency edges of every body.
    #[must_use]
    pub const fn flow(&self) -> &Flow {
        &self.flow
    }
}

impl Flow {
    /// Takes in one owner's gathered facts, numbering its closures after the
    /// ones already here.
    fn absorb_owner(
        &mut self,
        mut gathered: Gathered,
        store: impl FnOnce(&mut Self, Facts),
    ) {
        let base = self.closures.len();
        gathered.facts.rebase(base);
        for (_, facts) in &mut gathered.closures {
            facts.rebase(base);
        }
        self.closures.append(&mut gathered.closures);
        store(self, gathered.facts);
    }
}

/// Whether the default body `default` of an interface member is a host
/// binding or a grant row of the compiler intrinsic registry. Such a binding
/// is the implementation of a sealed grant only, so for an implementation
/// the member is abstract: it supplies the member, and one that does not is
/// rejected (`docs/spec/03-effects.md`, "Operation interfaces",
/// "Narrowing").
fn host_bound(tables: &Tables<'_>, default: Option<usize>) -> bool {
    default
        .and_then(|index| tables.function(index).ok())
        .is_some_and(|function| match &function.body {
            FunctionBody::Host(_) => true,
            FunctionBody::Intrinsic { intrinsic, .. } => matches!(
                intrinsic,
                CompilerIntrinsic::GrantRestrict
                    | CompilerIntrinsic::GrantSub
                    | CompilerIntrinsic::GrantLimit
            ),
            FunctionBody::Expr(_) => false,
        })
}

fn table_bool(table: &TypeTable) -> Result<Type, IrError> {
    table.library().declared(Library::Bool, Vec::new())
}

/// The types a declaration's body holds inline: its fields, components,
/// representation, or case fields.
fn body_components(definition: &TypeDefinition) -> Vec<&Type> {
    match definition.body() {
        TypeBody::Record(fields) => fields.iter().map(|(_, ty)| ty).collect(),
        TypeBody::Tuple(values) => values.iter().collect(),
        TypeBody::Wrapper(value) => vec![value],
        TypeBody::Variant(cases) => cases
            .iter()
            .flat_map(|case| case.fields.iter().map(|(_, ty)| ty))
            .collect(),
    }
}

/// The declarations and binders that `ty` holds inline, given for each
/// declaration the parameters its own layout holds inline. A list, a
/// callable, and an interface value hold their contents behind an
/// indirection; a tuple and a record hold their components inline.
fn layout_heads<'a>(
    ty: &'a Type,
    inline: &BTreeMap<&TypeId, BTreeSet<usize>>,
    declarations: &mut BTreeSet<&'a TypeId>,
    binders: &mut BTreeSet<usize>,
) {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Tuple(_) | Type::Record(_) => pending.extend(ty.components()),
            Type::Declared(id, arguments) => {
                declarations.insert(id);
                if let Some(parameters) = inline.get(id) {
                    pending.extend(
                        parameters.iter().filter_map(|index| arguments.get(*index)),
                    );
                }
            }
            Type::Binder(index) => {
                binders.insert(*index);
            }
            _ => {}
        }
    }
}

/// The type chapter's "Recursive types": a declaration may reach itself only
/// through a list, a dict, or a callable type, so that every value of it has
/// a finite layout.
fn check_finite_size(table: &TypeTable) -> Result<(), IrError> {
    let mut inline: BTreeMap<&TypeId, BTreeSet<usize>> = table
        .types()
        .keys()
        .map(|id| (id, BTreeSet::new()))
        .collect();
    loop {
        let mut changed = false;
        for (id, definition) in table.types() {
            let mut declarations = BTreeSet::new();
            let mut binders = BTreeSet::new();
            for component in body_components(definition) {
                layout_heads(component, &inline, &mut declarations, &mut binders);
            }
            let entry = inline.entry(id).or_default();
            if *entry != binders {
                *entry = binders;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut edges: BTreeMap<&TypeId, BTreeSet<&TypeId>> = BTreeMap::new();
    for (id, definition) in table.types() {
        let mut declarations = BTreeSet::new();
        let mut binders = BTreeSet::new();
        for component in body_components(definition) {
            layout_heads(component, &inline, &mut declarations, &mut binders);
        }
        declarations.retain(|target| table.types().contains_key(*target));
        edges.insert(id, declarations);
    }
    let mut incoming: BTreeMap<&TypeId, usize> =
        edges.keys().map(|id| (*id, 0)).collect();
    for targets in edges.values() {
        for target in targets {
            *incoming.entry(target).or_default() += 1;
        }
    }
    let mut ready: Vec<&TypeId> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    while let Some(id) = ready.pop() {
        for target in edges.get(id).into_iter().flatten() {
            if let Some(count) = incoming.get_mut(target) {
                *count -= 1;
                if *count == 0 {
                    ready.push(target);
                }
            }
        }
        incoming.remove(id);
    }
    match incoming.keys().next() {
        Some(id) => Err(IrError::InvalidType(format!(
            "`{}` holds itself inline and has no finite size",
            id.path()
        ))),
        None => Ok(()),
    }
}

/// Whether every `Self` in the result of the callable type `member` stands
/// in a position of the type chapter's `Self` map: the result itself, and
/// recursively a component of a tuple or record, or an argument of
/// `Option`, `Result`, or `List`.
fn self_map_result(member: &Type, library: &LibraryTypes) -> bool {
    let result = match member {
        Type::Fn(signature) => &signature.result,
        Type::Proc(signature) => &signature.result,
        _ => return true,
    };
    let mut pending = vec![result];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::SelfType => {}
            Type::Tuple(_) | Type::Record(_) | Type::List(_) => {
                pending.extend(ty.components());
            }
            Type::Declared(id, arguments)
                if matches!(
                    library.library_of(id),
                    Some(Library::Option | Library::Result)
                ) =>
            {
                pending.extend(arguments);
            }
            other => {
                let mut found = false;
                other.visit(&mut |inner| found |= *inner == Type::SelfType);
                if found {
                    return false;
                }
            }
        }
    }
    true
}

/// Whether two implementations of one interface have keys some types
/// satisfy at once, their binders renamed apart.
fn overlaps(left: &Implementation, right: &Implementation) -> bool {
    let offset = left.binders().len();
    let shifted = (0..right.binders().len())
        .map(|index| Type::Binder(offset.saturating_add(index)))
        .collect::<Vec<_>>();
    let rename = |ty: &Type| ty.instantiate(&shifted);
    let mut assignment = vec![None; offset.saturating_add(right.binders().len())];
    unify(left.receiver(), &rename(right.receiver()), &mut assignment)
        && left
            .contract()
            .arguments
            .iter()
            .zip(&right.contract().arguments)
            .all(|(left, right)| unify(left, &rename(right), &mut assignment))
}
