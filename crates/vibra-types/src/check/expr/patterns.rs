//! Patterns: their typing against a scrutinee type, their binders, and their
//! lowering.
//!
//! `docs/spec/01-source-language.md`, "Patterns" and the grammar note on
//! constructor patterns, and `docs/spec/02-type-system.md`, "Constant
//! expressions and constant patterns". A pattern is typed against the type
//! of the value it takes apart. A constructor pattern whose shape does not
//! fit the constructor it names is `@type.argumentMismatch` at the pattern;
//! any other pattern that does not fit its type is `@type.mismatch`. A
//! constant pattern expands to the pattern of its value; a value that is or
//! holds a float, `void`, or a dict is `@pattern.invalidConstant`. Each
//! pattern also yields the shape [`Pat`] that coverage reads.
//!
//! Patterns nest without bound, so a pattern is first walked top-down with
//! a work list into a flat list of levels, each typed against the type its
//! parent gives it, and then assembled bottom-up.

use std::rc::Rc;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{Constant, Library, Pattern as IrPattern, Type, Value};
use vibra_syntax::CstNode;
use vibra_syntax::source::{Ident, Pattern, PatternForm};

use super::{BodyChecker, Local};
use crate::check::declare::{Entity, Program, TypeKind};
use crate::check::literal::{self, LiteralFault};
use crate::check::pattern::{Ctor, Literal, Pat, Space, space};

/// A typed pattern.
#[derive(Debug)]
pub(super) struct Typed {
    /// Its shape for coverage.
    pub(super) cover: Rc<Pat>,
    /// Its lowering, when no fault was found in it.
    pub(super) lowered: Option<IrPattern>,
    /// Its binders, in written order, each with a fresh slot.
    pub(super) bindings: Vec<Local>,
}

/// What one level of a pattern is, once typed.
#[derive(Debug)]
enum Kind {
    /// A fault, reported or a name resolution already reported.
    Error,
    Discard,
    Bind(usize),
    Literal(Constant, Rc<Pat>),
    /// A wrapper of a declared type coverage splits.
    Wrapper,
    /// A wrapper of `Str`, `Bytes`, or `Dict`, whose representation coverage
    /// does not take apart.
    LibraryWrapper,
    /// A tuple or declared tuple of this width.
    Tuple,
    /// A record or declared record: the written fields, the position of each
    /// in declaration order, and the number of fields.
    Record {
        names: Vec<String>,
        positions: Vec<usize>,
        width: usize,
    },
    /// A case: its name, its position, and its written fields as a record's.
    Case {
        name: String,
        at: usize,
        names: Vec<String>,
        positions: Vec<usize>,
        width: usize,
    },
    List,
    /// A constant pattern before the constants are known: it covers nothing
    /// but itself and is not lowered.
    Provisional,
}

#[derive(Debug)]
struct Level {
    kind: Kind,
    children: Vec<Option<usize>>,
}

/// The sub-patterns of one level, each with the type it is checked against.
type Children<'a> = Vec<(Pattern<'a>, Option<Type>)>;

/// A pattern still to type, its type, and the level and position of its
/// parent.
type Work<'a> = (Pattern<'a>, Option<Type>, Option<(usize, usize)>);

impl<'a> BodyChecker<'_, 'a> {
    /// Types `pattern`, written at `span`, against a value of `subject`;
    /// `None` is the error type, against which nothing more is reported.
    pub(super) fn type_pattern(
        &mut self,
        pattern: Pattern<'a>,
        span: ByteSpan,
        subject: Option<&Type>,
    ) -> Typed {
        let mut levels: Vec<Level> = Vec::new();
        let mut bindings = Vec::new();
        let mut failed = false;
        let mut pending: Vec<Work<'a>> = vec![(pattern, subject.cloned(), None)];
        while let Some((current, ty, parent)) = pending.pop() {
            let index = levels.len();
            if let Some((parent, position)) = parent
                && let Some(slot) = levels
                    .get_mut(parent)
                    .and_then(|level| level.children.get_mut(position))
            {
                *slot = Some(index);
            }
            let at = if parent.is_none() {
                span
            } else {
                pattern_span(current, self.text)
            };
            let (kind, children) = self.level(current, ty, at, &mut bindings);
            failed |= matches!(kind, Kind::Error);
            levels.push(Level {
                kind,
                children: vec![None; children.len()],
            });
            for (position, (child, child_type)) in
                children.into_iter().enumerate().rev()
            {
                pending.push((child, child_type, Some((index, position))));
            }
        }
        let mut covers: Vec<Option<Rc<Pat>>> = vec![None; levels.len()];
        let mut lowered: Vec<Option<IrPattern>> =
            (0..levels.len()).map(|_| None).collect();
        for (index, level) in levels.into_iter().enumerate().rev() {
            let mut child_covers = Vec::new();
            let mut child_lowered = Vec::new();
            for child in &level.children {
                child_covers.push(
                    child
                        .and_then(|child| covers.get_mut(child)?.take())
                        .unwrap_or_else(Pat::wild),
                );
                child_lowered
                    .push(child.and_then(|child| lowered.get_mut(child)?.take()));
            }
            let (cover, ir) = self.assemble(level.kind, child_covers, child_lowered);
            if let Some(slot) = covers.get_mut(index) {
                *slot = Some(cover);
            }
            if let Some(slot) = lowered.get_mut(index) {
                *slot = ir;
            }
        }
        let cover = covers
            .first_mut()
            .and_then(Option::take)
            .unwrap_or_else(Pat::wild);
        let lowered = if failed {
            None
        } else {
            lowered.first_mut().and_then(Option::take)
        };
        Typed {
            cover,
            lowered,
            bindings,
        }
    }

    /// One level of a pattern from its typed children.
    fn assemble(
        &mut self,
        kind: Kind,
        covers: Vec<Rc<Pat>>,
        lowered: Vec<Option<IrPattern>>,
    ) -> (Rc<Pat>, Option<IrPattern>) {
        let all = |lowered: Vec<Option<IrPattern>>| {
            lowered.into_iter().collect::<Option<Vec<_>>>()
        };
        let placed = |covers: Vec<Rc<Pat>>, positions: &[usize], width: usize| {
            let mut children = vec![Pat::wild(); width];
            for (cover, position) in covers.into_iter().zip(positions) {
                if let Some(slot) = children.get_mut(*position) {
                    *slot = cover;
                }
            }
            children
        };
        match kind {
            Kind::Error => (Pat::wild(), None),
            Kind::Discard => (Pat::wild(), Some(IrPattern::Discard)),
            Kind::Bind(slot) => (Pat::wild(), Some(IrPattern::Bind(slot))),
            Kind::Literal(constant, cover) => {
                (cover, Some(IrPattern::Literal(constant)))
            }
            Kind::Wrapper => (
                Pat::ctor(Ctor::Single, covers),
                all(lowered)
                    .and_then(|inner| inner.into_iter().next())
                    .map(|inner| IrPattern::Wrapper(Box::new(inner))),
            ),
            Kind::LibraryWrapper => {
                let cover = if covers
                    .iter()
                    .all(|cover| matches!(cover.as_ref(), Pat::Wild))
                {
                    Pat::wild()
                } else {
                    self.opaque = self.opaque.saturating_add(1);
                    Pat::ctor(Ctor::Literal(Literal::Opaque(self.opaque)), Vec::new())
                };
                let ir = all(lowered)
                    .and_then(|inner| inner.into_iter().next())
                    .map(|inner| IrPattern::Wrapper(Box::new(inner)));
                (cover, ir)
            }
            Kind::Tuple => (
                Pat::ctor(Ctor::Single, covers),
                all(lowered).map(IrPattern::Tuple),
            ),
            Kind::Record {
                names,
                positions,
                width,
            } => (
                Pat::ctor(Ctor::Single, placed(covers, &positions, width)),
                all(lowered).map(|fields| {
                    IrPattern::Record(names.into_iter().zip(fields).collect())
                }),
            ),
            Kind::Case {
                name,
                at,
                names,
                positions,
                width,
            } => (
                Pat::ctor(Ctor::Case(at), placed(covers, &positions, width)),
                all(lowered).map(|fields| IrPattern::Case {
                    case: name,
                    fields: names.into_iter().zip(fields).collect(),
                }),
            ),
            Kind::Provisional => {
                self.opaque = self.opaque.saturating_add(1);
                (
                    Pat::ctor(Ctor::Literal(Literal::Opaque(self.opaque)), Vec::new()),
                    None,
                )
            }
            Kind::List => {
                let length = covers.len();
                (
                    Pat::ctor(Ctor::List(length), covers),
                    all(lowered).map(IrPattern::List),
                )
            }
        }
    }

    /// Types the outermost level of `pattern` against `ty`, binding a binder
    /// to a fresh slot, and returns its sub-patterns with their types.
    fn level(
        &mut self,
        pattern: Pattern<'a>,
        ty: Option<Type>,
        span: ByteSpan,
        bindings: &mut Vec<Local>,
    ) -> (Kind, Children<'a>) {
        match pattern.form() {
            PatternForm::Discard => (Kind::Discard, Vec::new()),
            PatternForm::Literal(node) => {
                (self.literal_pattern(node, ty.as_ref(), span), Vec::new())
            }
            PatternForm::Path(path) => {
                let segments = path.segments();
                if let [name] = segments.as_slice() {
                    return (self.name_pattern(*name, ty, span, bindings), Vec::new());
                }
                let names = segments
                    .iter()
                    .map(|segment| segment.text())
                    .collect::<Vec<_>>();
                if let Some((owner, case)) = self.case_of(&names) {
                    let path_span = path_span(&segments, span);
                    return self.case_level(
                        (owner, &case),
                        &[],
                        &[],
                        ty,
                        span,
                        path_span,
                    );
                }
                let kind = match self.static_path(&names) {
                    Some(Entity::Value(index)) => {
                        self.constant_pattern(index, ty.as_ref(), span)
                    }
                    Some(_) => {
                        self.report(
                            DiagnosticCode::TypeMismatch,
                            span,
                            "this path names no constructor or constant",
                        );
                        Kind::Error
                    }
                    None => Kind::Error,
                };
                (kind, Vec::new())
            }
            PatternForm::Apply {
                path,
                positional,
                labelled,
            } => {
                let segments = path.segments();
                let names = segments
                    .iter()
                    .map(|segment| segment.text())
                    .collect::<Vec<_>>();
                let path_span = path_span(&segments, span);
                let unknown = || {
                    positional
                        .iter()
                        .copied()
                        .chain(labelled.iter().map(|(_, pattern)| *pattern))
                        .map(|pattern| (pattern, None))
                        .collect::<Vec<_>>()
                };
                if let Some((owner, case)) = self.case_of(&names) {
                    return self.case_level(
                        (owner, &case),
                        &positional,
                        &labelled,
                        ty,
                        span,
                        path_span,
                    );
                }
                match self.static_path(&names) {
                    Some(Entity::Type(index)) => self.construct_level(
                        index,
                        &positional,
                        &labelled,
                        ty,
                        span,
                        path_span,
                    ),
                    Some(_) => {
                        self.report(
                            DiagnosticCode::TypeMismatch,
                            span,
                            "this path names no constructor",
                        );
                        (Kind::Error, unknown())
                    }
                    None => (Kind::Error, unknown()),
                }
            }
            PatternForm::Tuple(items) => match &ty {
                Some(Type::Tuple(components)) if components.len() == items.len() => (
                    Kind::Tuple,
                    items
                        .into_iter()
                        .zip(components.iter().cloned())
                        .map(|(item, component)| (item, Some(component)))
                        .collect(),
                ),
                _ => {
                    self.misfit(ty.as_ref(), span, "a tuple pattern");
                    (
                        Kind::Error,
                        items.into_iter().map(|item| (item, None)).collect(),
                    )
                }
            },
            PatternForm::Record(fields) => self.record_level(fields, ty, span),
            PatternForm::List(items) => match &ty {
                Some(Type::List(element)) => (
                    Kind::List,
                    items
                        .into_iter()
                        .map(|item| (item, Some(element.as_ref().clone())))
                        .collect(),
                ),
                _ => {
                    self.misfit(ty.as_ref(), span, "a list pattern");
                    (
                        Kind::Error,
                        items.into_iter().map(|item| (item, None)).collect(),
                    )
                }
            },
        }
    }

    /// Reports a pattern of the form `what` that does not fit `ty`.
    fn misfit(&mut self, ty: Option<&Type>, span: ByteSpan, what: &str) {
        if let Some(ty) = ty {
            self.report(
                DiagnosticCode::TypeMismatch,
                span,
                format!("{what} does not fit a value of type `{ty}`"),
            );
        }
    }

    /// A bare name: a binder, or a constant pattern when it resolves to a
    /// constant module value.
    fn name_pattern(
        &mut self,
        name: Ident<'a>,
        ty: Option<Type>,
        span: ByteSpan,
        bindings: &mut Vec<Local>,
    ) -> Kind {
        if self.local_index(name.text()).is_none() {
            match self.program.lookup(self.module, name.text()) {
                Some(Entity::Value(index)) => {
                    return self.constant_pattern(index, ty.as_ref(), span);
                }
                Some(Entity::Fn(_)) => return Kind::Error,
                _ => {}
            }
        }
        let slot = self.new_slot(ty.as_ref());
        bindings.push(Local {
            name: name.text().to_owned(),
            slot,
            ty,
            frame: self.frames.len(),
            generic: None,
        });
        Kind::Bind(slot)
    }

    /// A string, character, integer, or atom literal against `ty`.
    fn literal_pattern(
        &mut self,
        node: &CstNode,
        ty: Option<&Type>,
        span: ByteSpan,
    ) -> Kind {
        let Some(ty) = ty else {
            return Kind::Error;
        };
        let Some(written) = literal::read(node, self.text) else {
            return Kind::Error;
        };
        match literal::evaluate(&written, Some(ty), self.program.str_type.as_ref()) {
            Ok((own, value)) => self.literal_kind(own, value, ty, span),
            Err(LiteralFault::Mismatch(_)) => {
                self.misfit(Some(ty), span, "this literal pattern");
                Kind::Error
            }
            Err(LiteralFault::OutOfRange(own)) => {
                self.report(
                    DiagnosticCode::TypeNumericOutOfRange,
                    span,
                    format!("the literal does not fit `{own}`"),
                );
                Kind::Error
            }
            Err(LiteralFault::Ambiguous | LiteralFault::Malformed) => Kind::Error,
        }
    }

    /// The pattern of the literal `value` of type `own` against `ty`.
    fn literal_kind(
        &mut self,
        own: Type,
        value: Value,
        ty: &Type,
        span: ByteSpan,
    ) -> Kind {
        if own != *ty || matches!(own, Type::F32 | Type::F64 | Type::Void) {
            self.misfit(Some(ty), span, "this literal pattern");
            return Kind::Error;
        }
        let cover = match (&own, &value) {
            (Type::AtomSingleton(_), _) => Pat::ctor(Ctor::Single, Vec::new()),
            (_, value) => match literal_key(value) {
                Some(key) => Pat::ctor(Ctor::Literal(key), Vec::new()),
                None => {
                    self.misfit(Some(ty), span, "this literal pattern");
                    return Kind::Error;
                }
            },
        };
        match Constant::new(own, value) {
            Ok(constant) => Kind::Literal(constant, cover),
            Err(_) => Kind::Error,
        }
    }

    /// A constant pattern: the module value `index`, as its expanded value.
    /// Once the constants are known it is the pattern of the value the
    /// module value's initializer folds to; before, in the pass that finds
    /// them, a fieldless case and a literal are read from the initializer's
    /// text and any other constant is a pattern coverage cannot take apart.
    fn constant_pattern(
        &mut self,
        index: usize,
        ty: Option<&Type>,
        span: ByteSpan,
    ) -> Kind {
        let program = self.program;
        let Some(data) = program.values.get(index) else {
            return Kind::Error;
        };
        let Some(own) = data.ty.clone() else {
            self.skipped(data.module, span);
            return Kind::Error;
        };
        if !self.layout.constants_known {
            return self.provisional_constant(index, own, ty, span);
        }
        let value = self
            .layout
            .globals
            .get(index)
            .copied()
            .flatten()
            .and_then(|global| self.layout.constants.get(&global));
        let Some(value) = value.cloned() else {
            let related = data.view.name().map_or(span, |name| name.span());
            self.report_related(
                DiagnosticCode::NameRedeclaration,
                span,
                format!(
                    "`{}` names a module value whose initializer is not constant: \
                     it is neither a binder nor a constant pattern",
                    data.name
                ),
                data.module,
                related,
                "the module value",
            );
            return Kind::Error;
        };
        let Some(ty) = ty else {
            return Kind::Error;
        };
        if own != *ty {
            self.misfit(Some(ty), span, "this constant pattern");
            return Kind::Error;
        }
        let cover = match self.expand(&own, &value) {
            Ok(cover) => cover,
            Err(what) => {
                self.report(
                    DiagnosticCode::PatternInvalidConstant,
                    span,
                    format!(
                        "the constant `{}` holds {what}, which has no pattern form",
                        data.name
                    ),
                );
                return Kind::Error;
            }
        };
        match Constant::new(own, value) {
            Ok(constant) => Kind::Literal(constant, cover),
            Err(_) => Kind::Error,
        }
    }

    /// A constant pattern before the constants are known: a fieldless case or
    /// a literal read from the initializer's text, or a pattern no other
    /// pattern covers, left unlowered for the pass that knows the constants.
    fn provisional_constant(
        &mut self,
        index: usize,
        own: Type,
        ty: Option<&Type>,
        span: ByteSpan,
    ) -> Kind {
        let program = self.program;
        let Some(ty) = ty else {
            return Kind::Error;
        };
        if own != *ty {
            self.misfit(Some(ty), span, "this constant pattern");
            return Kind::Error;
        }
        if let Some((owner, case)) = program.constant_case(index)
            && let Some(at) = program
                .types
                .get(owner)
                .and_then(|data| data.cases())
                .and_then(|cases| cases.iter().position(|known| known.name == case))
        {
            let value = Value::Variant {
                case,
                fields: Vec::new(),
            };
            return match Constant::new(own, value) {
                Ok(constant) => {
                    Kind::Literal(constant, Pat::ctor(Ctor::Case(at), Vec::new()))
                }
                Err(_) => Kind::Error,
            };
        }
        let written = program
            .constant_literal(index)
            .and_then(|node| literal::read(node, self.module_text(index)?));
        if let Some(written) = written
            && let Ok((own, value)) =
                literal::evaluate(&written, Some(&own), program.str_type.as_ref())
            && let Some(key) = literal_key(&value)
        {
            let cover = if matches!(own, Type::AtomSingleton(_)) {
                Pat::ctor(Ctor::Single, Vec::new())
            } else {
                Pat::ctor(Ctor::Literal(key), Vec::new())
            };
            return match Constant::new(own, value) {
                Ok(constant) => Kind::Literal(constant, cover),
                Err(_) => Kind::Error,
            };
        }
        Kind::Provisional
    }

    /// The pattern of the constant `value` of type `ty`, as coverage sees
    /// it: a string, character, integer, or atom is its literal; a tuple,
    /// record, or list the tuple, record, or list pattern of its components'
    /// patterns; a case or declared type the constructor pattern of its
    /// fields' patterns. What a value holds that has no pattern form, a
    /// float, `void`, or a dict, is the error. Constants nest without bound,
    /// so the value is walked with a work list and assembled bottom-up.
    fn expand(&mut self, ty: &Type, value: &Value) -> Result<Rc<Pat>, &'static str> {
        let program = self.program;
        let mut shapes: Vec<(Ctor, Vec<usize>)> = Vec::new();
        let mut pending = vec![(ty.clone(), value, None::<(usize, usize)>)];
        while let Some((ty, value, parent)) = pending.pop() {
            let index = shapes.len();
            if let Some((parent, position)) = parent
                && let Some(slot) = shapes
                    .get_mut(parent)
                    .and_then(|(_, children)| children.get_mut(position))
            {
                *slot = index;
            }
            let (ctor, children) = expand_level(program, &ty, value)?;
            let ctor = match ctor {
                Some(ctor) => ctor,
                None => {
                    self.opaque = self.opaque.saturating_add(1);
                    Ctor::Literal(Literal::Opaque(self.opaque))
                }
            };
            shapes.push((ctor, vec![0; children.len()]));
            for (position, (child_type, child)) in
                children.into_iter().enumerate().rev()
            {
                pending.push((child_type, child, Some((index, position))));
            }
        }
        let mut built: Vec<Option<Rc<Pat>>> = vec![None; shapes.len()];
        for (index, (ctor, children)) in shapes.into_iter().enumerate().rev() {
            let children = children
                .iter()
                .map(|child| {
                    built
                        .get_mut(*child)
                        .and_then(Option::take)
                        .unwrap_or_else(Pat::wild)
                })
                .collect();
            if let Some(slot) = built.get_mut(index) {
                *slot = Some(Pat::ctor(ctor, children));
            }
        }
        Ok(built
            .first_mut()
            .and_then(Option::take)
            .unwrap_or_else(Pat::wild))
    }
    /// The text of the module that declares the module value `index`.
    fn module_text(&self, index: usize) -> Option<&'a str> {
        let module = self.program.values.get(index)?.module;
        self.program.modules.get(module)?.text
    }

    /// A case pattern, `Owner.case(…)` or `Owner.case`, of the case `case`
    /// of the type `owner`.
    fn case_level(
        &mut self,
        (owner, case): (usize, &str),
        positional: &[Pattern<'a>],
        labelled: &[(Ident<'a>, Pattern<'a>)],
        ty: Option<Type>,
        span: ByteSpan,
        path_span: ByteSpan,
    ) -> (Kind, Children<'a>) {
        let program = self.program;
        let unknown = || all_unknown(positional, labelled);
        let Some(data) = program.types.get(owner) else {
            return (Kind::Error, unknown());
        };
        let Some((at, declared)) = data.cases().and_then(|cases| {
            cases
                .iter()
                .enumerate()
                .find(|(_, known)| known.name == case)
                .map(|(at, known)| (at, known.fields.clone()))
        }) else {
            return (Kind::Error, unknown());
        };
        let fields = declared.unwrap_or_default();
        let fieldless = data.case(case).is_some_and(|known| known.fields.is_none());
        let shape = if fieldless {
            (positional.is_empty() && labelled.is_empty())
                .then(|| (Vec::new(), Vec::new()))
                .ok_or_else(|| format!("the case `{case}` has no fields"))
        } else if positional.is_empty() {
            labelled_shape(&fields, labelled)
        } else {
            Err(format!(
                "the case `{case}` has fields, so each sub-pattern names its field"
            ))
        };
        let (names, positions) = match shape {
            Ok(shape) => shape,
            Err(message) => {
                self.report(DiagnosticCode::TypeArgumentMismatch, span, message);
                return (Kind::Error, unknown());
            }
        };
        let Some(arguments) = self.fit(owner, ty.as_ref(), span) else {
            return (Kind::Error, unknown());
        };
        if self.hidden(owner, path_span) {
            return (Kind::Error, unknown());
        }
        let children = labelled
            .iter()
            .zip(&positions)
            .map(|((_, pattern), position)| {
                let field = fields
                    .get(*position)
                    .map(|(_, field)| field.instantiate(&arguments));
                (*pattern, field)
            })
            .collect();
        (
            Kind::Case {
                name: case.to_owned(),
                at,
                names,
                positions,
                width: fields.len(),
            },
            children,
        )
    }

    /// A pattern of a wrapper, declared tuple, or declared record.
    fn construct_level(
        &mut self,
        index: usize,
        positional: &[Pattern<'a>],
        labelled: &[(Ident<'a>, Pattern<'a>)],
        ty: Option<Type>,
        span: ByteSpan,
        path_span: ByteSpan,
    ) -> (Kind, Children<'a>) {
        let program = self.program;
        let unknown = || all_unknown(positional, labelled);
        let Some(data) = program.types.get(index) else {
            return (Kind::Error, unknown());
        };
        let name = data.name.clone();
        let positional_shape = |expected: usize| {
            if !labelled.is_empty() {
                Err(format!(
                    "`{name}` has no field names, so its sub-patterns are positional"
                ))
            } else if positional.len() == expected {
                Ok(())
            } else {
                Err(format!(
                    "`{name}` has {expected} component(s), but {} sub-pattern(s) are written",
                    positional.len()
                ))
            }
        };
        let (shape, components) = match &data.kind {
            TypeKind::Wrapper(representation) => (
                positional_shape(1).map(|()| None),
                vec![representation.clone()],
            ),
            TypeKind::Tuple(components) => (
                positional_shape(components.len()).map(|()| None),
                components.clone(),
            ),
            TypeKind::Record(fields) => {
                let shape = if positional.is_empty() {
                    labelled_shape(fields, labelled).map(Some)
                } else {
                    Err(format!(
                        "`{name}` has field names, so each sub-pattern names its field"
                    ))
                };
                (
                    shape,
                    fields.iter().map(|(_, field)| field.clone()).collect(),
                )
            }
            TypeKind::Unsupported => {
                self.skipped(data.module, path_span);
                return (Kind::Error, unknown());
            }
            TypeKind::Faulty | TypeKind::Pending => return (Kind::Error, unknown()),
            _ => {
                self.report(
                    DiagnosticCode::TypeMismatch,
                    span,
                    format!("the type `{name}` has no constructor of its own"),
                );
                return (Kind::Error, unknown());
            }
        };
        let record = match shape {
            Ok(record) => record,
            Err(message) => {
                self.report(DiagnosticCode::TypeArgumentMismatch, span, message);
                return (Kind::Error, unknown());
            }
        };
        let Some(arguments) = self.fit(index, ty.as_ref(), span) else {
            return (Kind::Error, unknown());
        };
        if self.hidden(index, path_span) {
            return (Kind::Error, unknown());
        }
        let instance = |at: usize| {
            components
                .get(at)
                .map(|component| component.instantiate(&arguments))
        };
        match record {
            Some((names, positions)) => {
                let children = labelled
                    .iter()
                    .zip(&positions)
                    .map(|((_, pattern), position)| (*pattern, instance(*position)))
                    .collect();
                (
                    Kind::Record {
                        names,
                        positions,
                        width: components.len(),
                    },
                    children,
                )
            }
            None => {
                let children = positional
                    .iter()
                    .enumerate()
                    .map(|(at, pattern)| (*pattern, instance(at)))
                    .collect();
                let library = data
                    .id
                    .as_ref()
                    .and_then(|id| program.library_of(id))
                    .is_some_and(|library| {
                        matches!(library, Library::Str | Library::Bytes | Library::Dict)
                    });
                let kind = match (&data.kind, library) {
                    (TypeKind::Wrapper(_), true) => Kind::LibraryWrapper,
                    (TypeKind::Wrapper(_), false) => Kind::Wrapper,
                    _ => Kind::Tuple,
                };
                (kind, children)
            }
        }
    }

    /// A structural record pattern against `ty`.
    fn record_level(
        &mut self,
        fields: Vec<(Ident<'a>, Pattern<'a>)>,
        ty: Option<Type>,
        span: ByteSpan,
    ) -> (Kind, Children<'a>) {
        let unknown = |fields: Vec<(Ident<'a>, Pattern<'a>)>| {
            fields
                .into_iter()
                .map(|(_, pattern)| (pattern, None))
                .collect()
        };
        let Some(Type::Record(declared)) = &ty else {
            self.misfit(ty.as_ref(), span, "a record pattern");
            return (Kind::Error, unknown(fields));
        };
        let entries = declared.entries().to_vec();
        match labelled_shape(&entries, &fields) {
            Ok((names, positions)) => {
                let children = fields
                    .iter()
                    .zip(&positions)
                    .map(|((_, pattern), position)| {
                        (
                            *pattern,
                            entries.get(*position).map(|(_, field)| field.clone()),
                        )
                    })
                    .collect();
                (
                    Kind::Record {
                        names,
                        positions,
                        width: entries.len(),
                    },
                    children,
                )
            }
            Err(_) => {
                self.misfit(ty.as_ref(), span, "this record pattern");
                (Kind::Error, unknown(fields))
            }
        }
    }

    /// The type arguments of `ty` when it is the declared type `index`,
    /// reporting a pattern of that type against any other.
    fn fit(
        &mut self,
        index: usize,
        ty: Option<&Type>,
        span: ByteSpan,
    ) -> Option<Vec<Type>> {
        let program = self.program;
        let data = program.types.get(index)?;
        let ty = ty?;
        match ty {
            Type::Declared(id, arguments) if data.id.as_ref() == Some(id) => {
                Some(arguments.clone())
            }
            _ => {
                self.report(
                    DiagnosticCode::TypeMismatch,
                    span,
                    format!(
                        "a pattern of `{}` does not fit a value of type `{ty}`",
                        data.name
                    ),
                );
                None
            }
        }
    }
}

/// The written field names of a labelled pattern and the position of each
/// in `declared`, or why they do not fit: an unknown or repeated field.
fn labelled_shape(
    declared: &[(String, Type)],
    labelled: &[(Ident<'_>, Pattern<'_>)],
) -> Result<(Vec<String>, Vec<usize>), String> {
    let mut names = Vec::new();
    let mut positions = Vec::new();
    for (label, _) in labelled {
        let Some(position) = declared.iter().position(|(name, _)| name == label.text())
        else {
            return Err(format!("there is no field `{}`", label.text()));
        };
        if positions.contains(&position) {
            return Err(format!("the field `{}` is matched twice", label.text()));
        }
        names.push(label.text().to_owned());
        positions.push(position);
    }
    Ok((names, positions))
}

/// Every sub-pattern of a faulty constructor pattern, against the error
/// type, so that its binders are still bound.
fn all_unknown<'a>(
    positional: &[Pattern<'a>],
    labelled: &[(Ident<'a>, Pattern<'a>)],
) -> Children<'a> {
    positional
        .iter()
        .copied()
        .chain(labelled.iter().map(|(_, pattern)| *pattern))
        .map(|pattern| (pattern, None))
        .collect()
}

/// One level of a constant's pattern: its head, `None` for one coverage does
/// not take apart, and its components with their types in declaration order.
type ConstantLevel<'v> = (Option<Ctor>, Vec<(Type, &'v Value)>);

/// One level of the pattern of the constant `value` of type `ty`, or what it
/// holds that has no pattern form.
fn expand_level<'v>(
    program: &Program<'_>,
    ty: &Type,
    value: &'v Value,
) -> Result<ConstantLevel<'v>, &'static str> {
    let in_order = |declared: &[(String, Type)], fields: &'v [(String, Value)]| {
        declared
            .iter()
            .map(|(name, field)| {
                fields
                    .iter()
                    .find(|(known, _)| known == name)
                    .map(|(_, value)| (field.clone(), value))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("a value its type does not describe")
    };
    let declared = || match ty {
        Type::Declared(id, arguments) => program
            .type_index_of(id)
            .and_then(|index| program.types.get(index))
            .map(|data| (data, arguments)),
        _ => None,
    };
    match value {
        Value::F32(_) | Value::F64(_) => Err("a float"),
        Value::Void => Err("`void`"),
        Value::Dict(_) => Err("a dict"),
        Value::Bytes(_) => Ok((None, Vec::new())),
        Value::Tuple(items) => {
            let components = match space(program, ty) {
                Space::Product(components) => components,
                _ => Vec::new(),
            };
            if components.len() != items.len() {
                return Err("a value its type does not describe");
            }
            Ok((
                Some(Ctor::Single),
                components.into_iter().zip(items).collect(),
            ))
        }
        Value::List(items) => match ty {
            Type::List(element) => Ok((
                Some(Ctor::List(items.len())),
                items
                    .iter()
                    .map(|item| (element.as_ref().clone(), item))
                    .collect(),
            )),
            _ => Err("a value its type does not describe"),
        },
        Value::Record(fields) => {
            let declared_fields = match (ty, declared()) {
                (Type::Record(named), _) => named.entries().to_vec(),
                (_, Some((data, arguments))) => match &data.kind {
                    TypeKind::Record(declared) => declared
                        .iter()
                        .map(|(name, field)| {
                            (name.clone(), field.instantiate(arguments))
                        })
                        .collect(),
                    _ => return Err("a value its type does not describe"),
                },
                _ => return Err("a value its type does not describe"),
            };
            Ok((Some(Ctor::Single), in_order(&declared_fields, fields)?))
        }
        Value::Variant { case, fields } => {
            let (data, arguments) =
                declared().ok_or("a value its type does not describe")?;
            let (at, known) = data
                .cases()
                .and_then(|cases| {
                    cases
                        .iter()
                        .enumerate()
                        .find(|(_, known)| known.name == *case)
                })
                .ok_or("a value its type does not describe")?;
            let declared_fields = known
                .fields
                .iter()
                .flatten()
                .map(|(name, field)| (name.clone(), field.instantiate(arguments)))
                .collect::<Vec<_>>();
            Ok((Some(Ctor::Case(at)), in_order(&declared_fields, fields)?))
        }
        Value::Wrapper(inner) => {
            let (data, arguments) =
                declared().ok_or("a value its type does not describe")?;
            match &data.kind {
                TypeKind::Wrapper(representation) => Ok((
                    Some(Ctor::Single),
                    vec![(representation.instantiate(arguments), &**inner)],
                )),
                _ => Err("a value its type does not describe"),
            }
        }
        _ => match (ty, literal_key(value)) {
            (Type::AtomSingleton(_), _) => Ok((Some(Ctor::Single), Vec::new())),
            (_, Some(key)) => Ok((Some(Ctor::Literal(key)), Vec::new())),
            (_, None) => Err("a value with no pattern form"),
        },
    }
}

/// The key coverage compares a literal value by.
fn literal_key(value: &Value) -> Option<Literal> {
    Some(match value {
        Value::I8(value) => Literal::Integer(i128::from(*value)),
        Value::I16(value) => Literal::Integer(i128::from(*value)),
        Value::I32(value) => Literal::Integer(i128::from(*value)),
        Value::I64(value) => Literal::Integer(i128::from(*value)),
        Value::U8(value) => Literal::Integer(i128::from(*value)),
        Value::U16(value) => Literal::Integer(i128::from(*value)),
        Value::U32(value) => Literal::Integer(i128::from(*value)),
        Value::U64(value) => Literal::Integer(i128::from(*value)),
        Value::Char(value) => Literal::Char(*value),
        Value::Str(value) => Literal::Str(value.clone()),
        Value::Atom(value) => Literal::Atom(value.clone()),
        _ => return None,
    })
}

/// The span of the path of a constructor pattern.
fn path_span(segments: &[Ident<'_>], fallback: ByteSpan) -> ByteSpan {
    match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => {
            ByteSpan::new(first.span().start(), last.span().end())
        }
        _ => fallback,
    }
}

/// The span of a pattern of the document `text`: its node, and for an
/// applied constructor through its closing parenthesis.
pub(super) fn pattern_span(pattern: Pattern<'_>, text: &str) -> ByteSpan {
    let span = pattern.node().span();
    if !matches!(pattern.form(), PatternForm::Apply { .. }) {
        return span;
    }
    closing_paren(text, span.end()).map_or(span, |end| ByteSpan::new(span.start(), end))
}

/// The end of the parenthesized group that opens after `from`, skipping
/// whitespace and comments before it and strings, characters, and comments
/// inside it.
fn closing_paren(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = from;
    let mut depth = 0usize;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while bytes.get(at).is_some_and(|byte| *byte != b'\n') {
                    at += 1;
                }
                continue;
            }
            b'"' | b'\'' if depth > 0 => {
                at += 1;
                while let Some(&inner) = bytes.get(at) {
                    at += 1;
                    if inner == b'\\' {
                        at += 1;
                    } else if inner == byte || inner == b'\n' {
                        break;
                    }
                }
                continue;
            }
            b'(' => depth += 1,
            b')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ if depth == 0 => return None,
            _ => {}
        }
        at += 1;
    }
    None
}
