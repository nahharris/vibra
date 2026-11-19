//! Coverage: exhaustiveness, reachability, and irrefutability.
//!
//! `docs/spec/02-type-system.md`, "Coverage", "Constant expressions and
//! constant patterns" (a constant pattern is covered as its expanded value),
//! and "`return` and `Never`" (which types are uninhabited). A `match` covers
//! its subject when no value is left that no arm matches; an arm is reachable
//! when some value reaches it past the arms before it; and a binding pattern
//! is irrefutable when it alone covers its type.
//!
//! All three are one question, usefulness: does a pattern match a value that
//! no row of a matrix of earlier patterns matches. The search splits a column
//! over the constructors of its type in declaration order, so the witness it
//! returns is the first uncovered shape in that order, with `_` for a
//! position only a binder can cover. Patterns and types nest without bound,
//! so the search is a machine over an explicit stack of frames, and it gives
//! up with [`LimitReached`] after [`COVERAGE_LIMIT`] units of work rather
//! than assume an answer.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use vibra_ir::{Library, Type};

use super::declare::{Program, TypeKind};

/// The work one `match`, `let`, or parameter pattern may take: one unit for
/// each matrix the search builds and one for each row it looks at. A
/// question that needs more is `@pattern.unsupportedCoverage`.
pub(crate) const COVERAGE_LIMIT: usize = 1 << 22;

/// A literal a pattern matches, as the search compares it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Literal {
    /// An integer of any width.
    Integer(i128),
    /// A `Char`.
    Char(char),
    /// A `Str`.
    Str(String),
    /// An atom on `Atom`, without its `@`.
    Atom(String),
    /// A pattern the search does not take apart, by a number unique in its
    /// question: it covers nothing but itself, and only a binder covers it.
    Opaque(usize),
}

/// The head of a pattern that is not a binder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ctor {
    /// The one constructor of a tuple, record, declared tuple, declared
    /// record, or wrapper, and the one atom of an atom singleton type.
    Single,
    /// The case of a variant type at this position in declaration order.
    Case(usize),
    /// A literal of a type literals never cover.
    Literal(Literal),
    /// A list of exactly this length.
    List(usize),
}

/// A pattern as coverage sees it: a binder or `_`, or a constructor with one
/// sub-pattern for each component of its type, in declaration order.
#[derive(Debug)]
pub(crate) enum Pat {
    /// Matches every value.
    Wild,
    /// Matches the values built by the constructor whose components match.
    Ctor(Ctor, Vec<Rc<Pat>>),
}

/// Dropping a pattern moves the sub-patterns it alone holds to a work list,
/// so that no host stack grows with its depth.
impl Drop for Pat {
    fn drop(&mut self) {
        let Self::Ctor(_, children) = self else {
            return;
        };
        let mut pending = std::mem::take(children);
        while let Some(child) = pending.pop() {
            if let Ok(mut pattern) = Rc::try_unwrap(child)
                && let Self::Ctor(_, grand) = &mut pattern
            {
                pending.append(grand);
            }
        }
    }
}

impl Pat {
    /// `_`.
    pub(crate) fn wild() -> Rc<Self> {
        Rc::new(Self::Wild)
    }

    /// `ctor` over `children`.
    pub(crate) fn ctor(ctor: Ctor, children: Vec<Rc<Self>>) -> Rc<Self> {
        Rc::new(Self::Ctor(ctor, children))
    }

    const fn is_wild(&self) -> bool {
        matches!(self, Self::Wild)
    }
}

/// The values of a type as coverage splits them.
#[derive(Clone, Debug)]
pub(crate) enum Space {
    /// One constructor whose components have these types.
    Product(Vec<Type>),
    /// The cases of a variant type in declaration order, each with the types
    /// of its fields.
    Variant(Vec<Vec<Type>>),
    /// The one atom of an atom singleton type.
    Singleton,
    /// Lists of every length over this element type.
    List(Type),
    /// Values only a binder covers: numbers, `Char`, `Str`, `Bytes`, `Atom`,
    /// `Dict`, callables, interface values, and type parameters.
    Open,
}

impl Space {
    /// The component types of `ctor`.
    fn arity(&self, ctor: &Ctor) -> Vec<Type> {
        match (self, ctor) {
            (Self::Product(components), Ctor::Single) => components.clone(),
            (Self::Variant(cases), Ctor::Case(at)) => {
                cases.get(*at).cloned().unwrap_or_default()
            }
            (Self::List(element), Ctor::List(length)) => vec![element.clone(); *length],
            _ => Vec::new(),
        }
    }
}

/// The search gave up: the question needs more than [`COVERAGE_LIMIT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LimitReached;

/// The coverage of a `match` that the search decided: each unreachable arm
/// with the earliest arm by which it is covered, and the first uncovered
/// shape.
#[derive(Debug, Default)]
pub(crate) struct MatchCoverage {
    /// Each unreachable arm, by position, and the arm whose addition left
    /// nothing for it, when an arm did.
    pub(crate) unreachable: Vec<(usize, Option<usize>)>,
    /// The first value shape no arm matches, spelled as a pattern.
    pub(crate) uncovered: Option<String>,
}

/// A usefulness question: a matrix whose rows and query hold their first
/// column last, so a column is taken off the end.
#[derive(Clone)]
struct Problem {
    rows: Vec<Vec<Rc<Pat>>>,
    columns: Vec<Type>,
    query: Vec<Rc<Pat>>,
}

/// What a frame does with the answer to the question below it.
enum Frame {
    /// The query's head named `ctor`: the witness of the specialized
    /// question is rebuilt under it.
    Rebuild(Ctor, usize),
    /// The column is open: the witness of the default matrix gets `_`.
    Prepend,
    /// The query's head is `_` on a closed space: each constructor in
    /// declaration order until one leaves a witness.
    Split(Box<Split>),
}

/// A split of a question over the constructors of its first column.
struct Split {
    problem: Problem,
    space: Space,
    ctors: Vec<Ctor>,
    /// The position of the constructor to try next.
    next: usize,
}

enum Outcome {
    Answer(Option<Vec<Rc<Pat>>>),
    Descend(Frame, Problem),
}

/// Answers coverage questions about the types of one program.
pub(crate) struct Coverage<'p, 'a> {
    program: &'p Program<'a>,
    budget: usize,
    uninhabited: HashMap<Type, bool>,
}

impl<'p, 'a> Coverage<'p, 'a> {
    /// A fresh search over the types of `program`, with [`COVERAGE_LIMIT`]
    /// units of work.
    pub(crate) fn new(program: &'p Program<'a>) -> Self {
        Self {
            program,
            budget: COVERAGE_LIMIT,
            uninhabited: HashMap::new(),
        }
    }

    fn charge(&mut self, units: usize) -> Result<(), LimitReached> {
        self.budget = self.budget.checked_sub(units).ok_or(LimitReached)?;
        Ok(())
    }

    /// The values of `ty` as coverage splits them.
    pub(crate) fn space(&self, ty: &Type) -> Space {
        space(self.program, ty)
    }

    /// Whether `ty` has no value.
    pub(crate) fn is_uninhabited(&mut self, ty: &Type) -> bool {
        if let Some(known) = self.uninhabited.get(ty) {
            return *known;
        }
        let mut pending = vec![(ty.clone(), false)];
        let mut open = HashSet::new();
        while let Some((current, expanded)) = pending.pop() {
            if self.uninhabited.contains_key(&current) {
                continue;
            }
            let parts = Parts::of(self.program, &current);
            if expanded {
                open.remove(&current);
                let known =
                    |ty: &Type| self.uninhabited.get(ty).copied().unwrap_or(false);
                let empty = match &parts {
                    Parts::Never => true,
                    Parts::Leaf => false,
                    Parts::Product(components) => components.iter().any(known),
                    Parts::Variant(cases) => {
                        cases.iter().all(|fields| fields.iter().any(known))
                    }
                };
                self.uninhabited.insert(current, empty);
                continue;
            }
            if !open.insert(current.clone()) {
                // A cycle: a type that holds itself directly is rejected
                // for its infinite size, so nothing proves it empty here.
                self.uninhabited.insert(current, false);
                continue;
            }
            let children = match &parts {
                Parts::Product(components) => components.clone(),
                Parts::Variant(cases) => cases.iter().flatten().cloned().collect(),
                Parts::Never | Parts::Leaf => Vec::new(),
            };
            pending.push((current, true));
            pending.extend(
                children
                    .into_iter()
                    .filter(|child| !self.uninhabited.contains_key(child))
                    .map(|child| (child, false)),
            );
        }
        self.uninhabited.get(ty).copied().unwrap_or(false)
    }

    /// The first value of `ty` that no pattern of `rows` matches.
    pub(crate) fn uncovered(
        &mut self,
        rows: &[Rc<Pat>],
        ty: &Type,
    ) -> Result<Option<Rc<Pat>>, LimitReached> {
        let problem = Problem {
            rows: rows.iter().map(|row| vec![Rc::clone(row)]).collect(),
            columns: vec![ty.clone()],
            query: vec![Pat::wild()],
        };
        Ok(self.solve(problem)?.and_then(|mut witness| witness.pop()))
    }

    /// Whether `query` matches a value of `ty` that no pattern of `rows`
    /// matches.
    pub(crate) fn useful(
        &mut self,
        rows: &[Rc<Pat>],
        ty: &Type,
        query: &Rc<Pat>,
    ) -> Result<bool, LimitReached> {
        let problem = Problem {
            rows: rows.iter().map(|row| vec![Rc::clone(row)]).collect(),
            columns: vec![ty.clone()],
            query: vec![Rc::clone(query)],
        };
        Ok(self.solve(problem)?.is_some())
    }

    /// Whether `pattern` alone covers `ty`.
    pub(crate) fn irrefutable(
        &mut self,
        pattern: &Rc<Pat>,
        ty: &Type,
    ) -> Result<bool, LimitReached> {
        Ok(self.uncovered(std::slice::from_ref(pattern), ty)?.is_none())
    }

    /// The reachability of every arm of a `match` on `ty` and its first
    /// uncovered shape. An unreachable arm is related to the earliest arm by
    /// which the arms before it leave it nothing: the last arm of the
    /// shortest prefix it is not useful against.
    pub(crate) fn match_coverage(
        &mut self,
        arms: &[Rc<Pat>],
        ty: &Type,
    ) -> Result<MatchCoverage, LimitReached> {
        let mut coverage = MatchCoverage::default();
        for (at, arm) in arms.iter().enumerate() {
            let earlier = arms.get(..at).unwrap_or_default();
            if self.useful(earlier, ty, arm)? {
                continue;
            }
            // Usefulness only falls as rows are added, so the shortest
            // prefix that leaves the arm nothing is found by bisection.
            let (mut low, mut high) = (0, at);
            while low < high {
                let middle = low + (high - low) / 2;
                let prefix = arms.get(..middle).unwrap_or_default();
                if self.useful(prefix, ty, arm)? {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
            coverage.unreachable.push((at, low.checked_sub(1)));
        }
        coverage.uncovered = self
            .uncovered(arms, ty)?
            .map(|witness| spell(self.program, &witness, ty));
        Ok(coverage)
    }

    /// Answers one usefulness question, returning a witness row when the
    /// query is useful.
    fn solve(
        &mut self,
        problem: Problem,
    ) -> Result<Option<Vec<Rc<Pat>>>, LimitReached> {
        let mut frames: Vec<Frame> = Vec::new();
        let mut current = Some(problem);
        let mut answer = None;
        loop {
            if let Some(problem) = current.take() {
                match self.step(problem)? {
                    Outcome::Answer(found) => answer = found,
                    Outcome::Descend(frame, child) => {
                        frames.push(frame);
                        current = Some(child);
                        continue;
                    }
                }
            }
            let Some(frame) = frames.pop() else {
                return Ok(answer);
            };
            match frame {
                Frame::Rebuild(ctor, arity) => {
                    answer = answer.map(|witness| rebuild(witness, ctor, arity));
                }
                Frame::Prepend => {
                    answer = answer.map(|mut witness| {
                        witness.push(Pat::wild());
                        witness
                    });
                }
                Frame::Split(mut split) => {
                    let tried =
                        split.next.checked_sub(1).and_then(|at| split.ctors.get(at));
                    if let (Some(witness), Some(ctor)) = (answer.take(), tried) {
                        let arity = split.space.arity(ctor).len();
                        answer = Some(rebuild(witness, ctor.clone(), arity));
                        continue;
                    }
                    if let Some(ctor) = split.ctors.get(split.next).cloned() {
                        let child =
                            self.specialize(&split.problem, &split.space, &ctor)?;
                        split.next += 1;
                        frames.push(Frame::Split(split));
                        current = Some(child);
                    }
                }
            }
        }
    }

    /// One step of the search on `problem`.
    fn step(&mut self, problem: Problem) -> Result<Outcome, LimitReached> {
        self.charge(problem.rows.len().saturating_add(1))?;
        let Some(column) = problem.columns.last().cloned() else {
            let witness = problem.rows.is_empty().then(Vec::new);
            return Ok(Outcome::Answer(witness));
        };
        if problem
            .rows
            .iter()
            .any(|row| row.iter().all(|pattern| pattern.is_wild()))
        {
            return Ok(Outcome::Answer(None));
        }
        if self.is_uninhabited(&column) {
            return Ok(Outcome::Answer(None));
        }
        let space = self.space(&column);
        let head = problem.query.last().cloned().unwrap_or_else(Pat::wild);
        if let Pat::Ctor(ctor, _) = head.as_ref() {
            let arity = space.arity(ctor).len();
            let child = self.specialize(&problem, &space, ctor)?;
            return Ok(Outcome::Descend(Frame::Rebuild(ctor.clone(), arity), child));
        }
        let ctors = match &space {
            Space::Product(_) | Space::Singleton => vec![Ctor::Single],
            Space::Variant(cases) => {
                let mut inhabited = Vec::new();
                for (at, fields) in cases.iter().enumerate() {
                    if !fields.iter().any(|field| self.is_uninhabited(field)) {
                        inhabited.push(Ctor::Case(at));
                    }
                }
                inhabited
            }
            Space::List(_) | Space::Open => {
                let mut columns = problem.columns;
                columns.pop();
                let mut query = problem.query;
                query.pop();
                let rows = problem
                    .rows
                    .into_iter()
                    .filter_map(|mut row| {
                        let head = row.pop()?;
                        head.is_wild().then_some(row)
                    })
                    .collect();
                let child = Problem {
                    rows,
                    columns,
                    query,
                };
                return Ok(Outcome::Descend(Frame::Prepend, child));
            }
        };
        let Some(first) = ctors.first().cloned() else {
            return Ok(Outcome::Answer(None));
        };
        let child = self.specialize(&problem, &space, &first)?;
        let split = Split {
            problem,
            space,
            ctors,
            next: 1,
        };
        Ok(Outcome::Descend(Frame::Split(Box::new(split)), child))
    }

    /// `problem` restricted to the values built by `ctor`, whose components
    /// replace its first column.
    fn specialize(
        &mut self,
        problem: &Problem,
        space: &Space,
        ctor: &Ctor,
    ) -> Result<Problem, LimitReached> {
        self.charge(problem.rows.len().saturating_add(1))?;
        let arity = space.arity(ctor);
        let width = arity.len();
        let mut columns = problem.columns.clone();
        columns.pop();
        columns.extend(arity.into_iter().rev());
        let expand = |row: &[Rc<Pat>]| -> Option<Vec<Rc<Pat>>> {
            let (head, rest) = row.split_last()?;
            let mut next = rest.to_vec();
            match head.as_ref() {
                Pat::Wild => next.extend((0..width).map(|_| Pat::wild())),
                Pat::Ctor(own, children) if own == ctor => {
                    next.extend(children.iter().rev().cloned());
                    next.extend((children.len()..width).map(|_| Pat::wild()));
                }
                Pat::Ctor(..) => return None,
            }
            Some(next)
        };
        let rows = problem.rows.iter().filter_map(|row| expand(row)).collect();
        let query = expand(&problem.query).unwrap_or_default();
        Ok(Problem {
            rows,
            columns,
            query,
        })
    }
}

/// `witness` with its first `arity` columns taken under `ctor`.
fn rebuild(mut witness: Vec<Rc<Pat>>, ctor: Ctor, arity: usize) -> Vec<Rc<Pat>> {
    let children = (0..arity)
        .map(|_| witness.pop().unwrap_or_else(Pat::wild))
        .collect();
    witness.push(Pat::ctor(ctor, children));
    witness
}

/// What a type is built from, for deciding whether it has a value.
enum Parts {
    Never,
    Leaf,
    Product(Vec<Type>),
    Variant(Vec<Vec<Type>>),
}

impl Parts {
    fn of(program: &Program<'_>, ty: &Type) -> Self {
        if *ty == Type::Never {
            return Self::Never;
        }
        match space(program, ty) {
            Space::Product(components) => Self::Product(components),
            Space::Variant(cases) => Self::Variant(cases),
            Space::Singleton | Space::List(_) | Space::Open => Self::Leaf,
        }
    }
}

/// The values of `ty` as coverage splits them.
pub(crate) fn space(program: &Program<'_>, ty: &Type) -> Space {
    match ty {
        Type::Tuple(components) => Space::Product(components.clone()),
        Type::Record(fields) => Space::Product(
            fields
                .entries()
                .iter()
                .map(|(_, field)| field.clone())
                .collect(),
        ),
        Type::AtomSingleton(_) => Space::Singleton,
        Type::List(element) => Space::List(element.as_ref().clone()),
        Type::Declared(id, arguments) => {
            if matches!(
                program.library_of(id),
                Some(Library::Str | Library::Bytes | Library::Dict)
            ) {
                return Space::Open;
            }
            let Some(data) = program
                .type_index_of(id)
                .and_then(|index| program.types.get(index))
            else {
                return Space::Open;
            };
            match &data.kind {
                TypeKind::Wrapper(representation) => {
                    Space::Product(vec![representation.instantiate(arguments)])
                }
                TypeKind::Tuple(components) => Space::Product(
                    components
                        .iter()
                        .map(|component| component.instantiate(arguments))
                        .collect(),
                ),
                TypeKind::Record(fields) => Space::Product(
                    fields
                        .iter()
                        .map(|(_, field)| field.instantiate(arguments))
                        .collect(),
                ),
                TypeKind::Variant(cases) => Space::Variant(
                    cases
                        .iter()
                        .map(|case| {
                            case.fields
                                .iter()
                                .flatten()
                                .map(|(_, field)| field.instantiate(arguments))
                                .collect()
                        })
                        .collect(),
                ),
                _ => Space::Open,
            }
        }
        _ => Space::Open,
    }
}

/// One piece of a spelling still to write.
enum Spelling {
    Pattern(Rc<Pat>, Type),
    Text(String),
}

/// `pattern`, a pattern over `ty`, in pattern spelling: `_` for a binder,
/// a constructor by its name, a tuple, record, or list in its brackets.
pub(crate) fn spell(program: &Program<'_>, pattern: &Rc<Pat>, ty: &Type) -> String {
    let mut out = String::new();
    let mut pending = vec![Spelling::Pattern(Rc::clone(pattern), ty.clone())];
    while let Some(piece) = pending.pop() {
        let (pattern, ty) = match piece {
            Spelling::Text(text) => {
                out.push_str(&text);
                continue;
            }
            Spelling::Pattern(pattern, ty) => (pattern, ty),
        };
        let Pat::Ctor(ctor, children) = pattern.as_ref() else {
            out.push('_');
            continue;
        };
        let layout = Layout::of(program, ctor, &ty);
        let mut pieces = vec![Spelling::Text(layout.open)];
        for (at, child) in children.iter().enumerate() {
            if at > 0 {
                pieces.push(Spelling::Text(" ".to_owned()));
            }
            if let Some(Some(label)) = layout.labels.get(at) {
                pieces.push(Spelling::Text(format!("{label}: ")));
            }
            let child_type = layout.types.get(at).cloned().unwrap_or(Type::Void);
            pieces.push(Spelling::Pattern(Rc::clone(child), child_type));
        }
        pieces.push(Spelling::Text(layout.close));
        pending.extend(pieces.into_iter().rev());
    }
    out
}

/// How one constructor is spelled: what opens and closes it, and the label
/// and type of each component.
struct Layout {
    open: String,
    close: String,
    labels: Vec<Option<String>>,
    types: Vec<Type>,
}

impl Layout {
    fn of(program: &Program<'_>, ctor: &Ctor, ty: &Type) -> Self {
        let plain = |open: &str, close: &str, types: Vec<Type>| Self {
            open: open.to_owned(),
            close: close.to_owned(),
            labels: vec![None; types.len()],
            types,
        };
        match (ty, ctor) {
            (Type::Tuple(components), _) => plain("{", "}", components.clone()),
            (Type::Record(fields), _) => Self {
                open: "{".to_owned(),
                close: "}".to_owned(),
                labels: fields.names().map(|name| Some(name.to_owned())).collect(),
                types: fields
                    .entries()
                    .iter()
                    .map(|(_, field)| field.clone())
                    .collect(),
            },
            (Type::AtomSingleton(atom), _) => {
                plain(&format!("@{atom}"), "", Vec::new())
            }
            (Type::List(element), Ctor::List(length)) => {
                plain("[", "]", vec![element.as_ref().clone(); *length])
            }
            (_, Ctor::Literal(literal)) => {
                plain(&literal_spelling(literal), "", Vec::new())
            }
            (Type::Declared(id, arguments), _) => {
                let Some(data) = program
                    .type_index_of(id)
                    .and_then(|index| program.types.get(index))
                else {
                    return plain("_", "", Vec::new());
                };
                let name = &data.name;
                let named = |prefix: String, fields: &[(String, Type)]| Self {
                    open: format!("{prefix}("),
                    close: ")".to_owned(),
                    labels: fields.iter().map(|(name, _)| Some(name.clone())).collect(),
                    types: fields
                        .iter()
                        .map(|(_, field)| field.instantiate(arguments))
                        .collect(),
                };
                match (&data.kind, ctor) {
                    (TypeKind::Variant(cases), Ctor::Case(at)) => {
                        let Some(case) = cases.get(*at) else {
                            return plain("_", "", Vec::new());
                        };
                        named(
                            format!("{name}.{}", case.name),
                            case.fields.as_deref().unwrap_or_default(),
                        )
                    }
                    (TypeKind::Record(fields), _) => named(name.clone(), fields),
                    (TypeKind::Tuple(components), _) => plain(
                        &format!("{name}("),
                        ")",
                        components
                            .iter()
                            .map(|component| component.instantiate(arguments))
                            .collect(),
                    ),
                    (TypeKind::Wrapper(representation), _) => plain(
                        &format!("{name}("),
                        ")",
                        vec![representation.instantiate(arguments)],
                    ),
                    _ => plain("_", "", Vec::new()),
                }
            }
            _ => plain("_", "", Vec::new()),
        }
    }
}

fn literal_spelling(literal: &Literal) -> String {
    match literal {
        Literal::Integer(value) => value.to_string(),
        Literal::Char(value) => format!("{value:?}"),
        Literal::Str(value) => format!("{value:?}"),
        Literal::Atom(value) => format!("@{value}"),
        Literal::Opaque(_) => "_".to_owned(),
    }
}
