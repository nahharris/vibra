//! Registry rows: the rows that are a few instructions, lowered inline from
//! the table of [`crate::primitive`], and the looping rows, which are calls of
//! the native imports of `vibra_ir::external::NATIVE_ROWS`.
//!
//! Every operand is in a slot that owns it, and every row consumes its
//! operands: a native import copies what it keeps and takes its own counts, so
//! the operands are dropped once the row has run. A row whose meaning applies
//! a function value (`list.fold`) or returns a remainder (`list.next`,
//! `dict.next`) is unsupported, since a pure import cannot call back into the
//! module.

use vibra_ir::external::{
    Answer, CompilerIntrinsic, NativeRow, ObjectKind, Operand, Totality,
};
use vibra_ir::{SourceOrigin, Type};
use wasm_encoder::Instruction as I;

use super::{
    Builder, FRAME, OBJECT, Part, SCRATCH, Shape, Variant, WA, WR, cell_class,
};
use crate::error::EmitError;
use crate::layout::{self, CellClass, Kind};
use crate::primitive::{self, Names, Outcome, Plan};
use crate::runtime::{ValueClass, c32, c64, get, index, ld32, ld64, set, st64};

impl<'a> Builder<'_, 'a> {
    /// Applies the row `intrinsic` at the closed `type_arguments` to the
    /// operands in `operands`, which it consumes, into `dest`.
    pub(super) fn intrinsic_on(
        &mut self,
        intrinsic: CompilerIntrinsic,
        type_arguments: &[Type],
        operands: &[u32],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let set = self.lowering.set;
        let row = intrinsic.row();
        let instantiate = |ty: &vibra_ir::external::RowType| {
            ty.instantiate(set.library(), type_arguments)
                .map_err(|error| EmitError::malformed(error.to_string(), origin))
        };
        let parameters = row
            .parameters
            .iter()
            .map(instantiate)
            .collect::<Result<Vec<_>, _>>()?;
        let result = instantiate(&row.result)?;
        if parameters.len() != operands.len() {
            return Err(EmitError::malformed(
                format!("`{}` with {} operands", row.symbol, operands.len()),
                origin,
            ));
        }
        let classes = parameters
            .iter()
            .map(|ty| self.class(ty, origin))
            .collect::<Result<Vec<_>, _>>()?;
        match intrinsic {
            CompilerIntrinsic::AssertTrue
            | CompilerIntrinsic::AssertFalse
            | CompilerIntrinsic::AssertEqual => {
                return self.assertion(intrinsic, &parameters, operands, origin, dest);
            }
            CompilerIntrinsic::ListFold => {
                return Err(EmitError::unsupported("listFold", origin));
            }
            CompilerIntrinsic::ListNext => {
                return Err(EmitError::unsupported("listNext", origin));
            }
            CompilerIntrinsic::DictNext => {
                return Err(EmitError::unsupported("dictNext", origin));
            }
            _ => {}
        }
        if let Some(native) = intrinsic.native_import() {
            return self.native_on(native, operands, &classes, &result, origin, dest);
        }
        match primitive::plan(intrinsic) {
            Some(plan) => {
                self.primitive_on(&plan, operands, &classes, &result, origin, dest)
            }
            None => Err(EmitError::malformed(
                format!("the row `{}`", row.symbol),
                origin,
            )),
        }
    }

    /// Drops every operand that holds a reference.
    fn release_operands(&mut self, operands: &[u32], classes: &[ValueClass]) {
        for (slot, class) in operands.iter().zip(classes) {
            self.discard(*slot, *class);
        }
    }

    /// The type of the only field of the case `case` of the variant `ty`.
    fn payload_type(
        &self,
        ty: &Type,
        case: &str,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        self.shape(ty, origin)?
            .case(case)
            .and_then(|(_, fields)| fields.first())
            .map(|(_, field)| field.clone())
            .ok_or_else(|| {
                EmitError::malformed(format!("{ty} has no payload in `{case}`"), origin)
            })
    }

    /// The case indices a row's computation selects among, read from the
    /// declarations of its result type.
    fn names(
        &self,
        plan: &Plan,
        result: &Type,
        origin: &SourceOrigin,
    ) -> Result<Names, EmitError> {
        let mut names = Names::default();
        match plan.outcome() {
            Outcome::Checked(_) => {
                let error = self.payload_type(result, "err", origin)?;
                for fault in plan.faults() {
                    let case = self.case_index(&error, fault.case(), origin)?;
                    if let Some(name) = names.faults.get_mut(fault as usize) {
                        *name = case.cast_signed() + 1;
                    }
                }
            }
            Outcome::Ordering => {
                for (name, case) in
                    names.order.iter_mut().zip(["less", "equal", "greater"])
                {
                    *name = self.case_index(result, case, origin)?.cast_signed();
                }
            }
            Outcome::Bool => {
                for (name, case) in names.truth.iter_mut().zip(["false", "true"]) {
                    *name = self.case_index(result, case, origin)?.cast_signed();
                }
            }
            Outcome::Scalar(_) | Outcome::Saturated(_) | Outcome::Option => {}
        }
        Ok(names)
    }

    /// Maps a truth (`1` or `0`) in `SCRATCH` to the case index of `true` or
    /// `false` of the `Bool` type `boolean`.
    fn truth_to_case(
        &mut self,
        boolean: &Type,
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        let truth = self.case_index(boolean, "true", origin)?;
        let falsity = self.case_index(boolean, "false", origin)?;
        self.push(&[
            c32(truth.cast_signed()),
            c32(falsity.cast_signed()),
            get(SCRATCH),
            I::Select,
            set(SCRATCH),
        ]);
        Ok(())
    }

    /// A row that is a few instructions: the operands are loaded and computed
    /// into one value and one selector, and the result is built from them in
    /// the one way the row's outcome fixes.
    fn primitive_on(
        &mut self,
        plan: &Plan,
        operands: &[u32],
        classes: &[ValueClass],
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        if plan.operands().len() != operands.len() {
            return Err(EmitError::malformed("a primitive row's operands", origin));
        }
        let names = self.names(plan, result, origin)?;
        let offsets = operands
            .iter()
            .map(|slot| self.cell(*slot))
            .collect::<Vec<_>>();
        let code = plan.compute(&offsets, &names);
        self.push(&code);
        let outcome = plan.outcome();
        if !matches!(outcome, Outcome::Scalar(_) | Outcome::Saturated(_)) {
            self.push(&[set(SCRATCH)]);
        }
        self.release_operands(operands, classes);
        match outcome {
            Outcome::Scalar(_) | Outcome::Saturated(_) => {
                self.store_value(plan, dest);
            }
            Outcome::Bool => {
                self.truth_to_case(result, origin)?;
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: 0,
                    },
                    &[],
                    dest,
                );
            }
            Outcome::Ordering => {
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: 0,
                    },
                    &[],
                    dest,
                );
            }
            Outcome::Checked(_) => {
                let ok = self.case_index(result, "ok", origin)?;
                let err = self.case_index(result, "err", origin)?;
                let (good, bad, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.push(&[get(SCRATCH), I::I32Eqz]);
                self.end(super::Term::Branch {
                    then_block: good,
                    else_block: bad,
                });
                self.start(good);
                let value = self.temp();
                self.store_value(plan, value);
                self.build(
                    Kind::Variant,
                    Variant::Fixed(ok),
                    &[Part::Slot {
                        slot: value,
                        class: plan.class(),
                    }],
                    dest,
                );
                self.goto(join, bad);
                let error = self.temp();
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: -1,
                    },
                    &[],
                    error,
                );
                self.build(
                    Kind::Variant,
                    Variant::Fixed(err),
                    &[Part::Slot {
                        slot: error,
                        class: ValueClass::Ref,
                    }],
                    dest,
                );
                self.goto(join, join);
            }
            Outcome::Option => {
                // `char.fromU32`: `some` of the operand when it is a scalar.
                let some = self.case_index(result, "some", origin)?;
                let none = self.case_index(result, "none", origin)?;
                let (present, absent, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.push(&[get(SCRATCH)]);
                self.end(super::Term::Branch {
                    then_block: present,
                    else_block: absent,
                });
                self.start(present);
                let value = self.temp();
                let at = self.cell(value);
                self.push(&[
                    get(FRAME),
                    get(WA),
                    I::I32WrapI64,
                    I::I64ExtendI32U,
                    st64(at),
                ]);
                self.set_byte(value, CellClass::I32.code());
                self.build(
                    Kind::Variant,
                    Variant::Fixed(some),
                    &[Part::Slot {
                        slot: value,
                        class: ValueClass::I32,
                    }],
                    dest,
                );
                self.goto(join, absent);
                self.build(Kind::Variant, Variant::Fixed(none), &[], dest);
                self.goto(join, join);
            }
        }
        Ok(())
    }

    /// Stores the value a row computed in `slot`.
    fn store_value(&mut self, plan: &Plan, slot: u32) {
        let at = self.cell(slot);
        self.push(&[get(FRAME)]);
        self.push(&plan.value());
        self.push(&[st64(at)]);
        self.set_byte(slot, cell_class(plan.class()).code());
    }

    /// Pushes the scalars of the operands of an import: an offset, an index or
    /// a number, or a cell and its class byte.
    fn native_operands(
        &mut self,
        row: &NativeRow,
        operands: &[u32],
        classes: &[ValueClass],
    ) {
        for ((operand, slot), class) in row.operands.iter().zip(operands).zip(classes) {
            let at = self.cell(*slot);
            match operand {
                Operand::Text | Operand::Bytes | Operand::Cells => {
                    self.push(&[get(FRAME), ld32(at)]);
                }
                Operand::Index | Operand::Number(_) => {
                    self.push(&[get(FRAME), ld64(at)])
                }
                Operand::Element => {
                    self.push(&[
                        get(FRAME),
                        ld64(at),
                        c32(i32::from(cell_class(*class).code())),
                    ]);
                }
            }
        }
    }

    /// A call of a native import. A row that builds an object runs twice:
    /// once to measure, and once to fill the object the module allocated to
    /// that size.
    fn native_on(
        &mut self,
        row: NativeRow,
        operands: &[u32],
        classes: &[ValueClass],
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        if row.operands.len() != operands.len() {
            return Err(EmitError::malformed("a native row's operands", origin));
        }
        let import = self.lowering.import(row.intrinsic);
        match row.answer {
            Answer::Length => {
                let at = self.cell(dest);
                self.push(&[get(FRAME)]);
                self.native_operands(&row, operands, classes);
                self.push(&[I::Call(import), st64(at)]);
                self.set_byte(dest, CellClass::I64.code());
                self.release_operands(operands, classes);
            }
            Answer::Same => {
                self.native_operands(&row, operands, classes);
                self.push(&[I::Call(import), set(SCRATCH)]);
                self.release_operands(operands, classes);
                self.truth_to_case(result, origin)?;
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: 0,
                    },
                    &[],
                    dest,
                );
            }
            Answer::Order => {
                let mut order = [0_i32; 3];
                for (name, case) in order.iter_mut().zip(["less", "equal", "greater"]) {
                    *name = self.case_index(result, case, origin)?.cast_signed();
                }
                let [less, equal, greater] = order;
                self.native_operands(&row, operands, classes);
                self.push(&[
                    I::Call(import),
                    set(SCRATCH),
                    c32(less),
                    c32(equal),
                    c32(greater),
                    get(SCRATCH),
                    I::I32Eqz,
                    I::Select,
                    get(SCRATCH),
                    c32(0),
                    I::I32LtS,
                    I::Select,
                    set(SCRATCH),
                ]);
                self.release_operands(operands, classes);
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: 0,
                    },
                    &[],
                    dest,
                );
            }
            Answer::Parsed => {
                // The import writes the number into a cell of this frame and
                // answers `0`, or answers `1` for `invalidFormat` and `2` for
                // `outOfRange`.
                let number = self.payload_type(result, "ok", origin)?;
                let class = self.class(&number, origin)?;
                let error_type = self.payload_type(result, "err", origin)?;
                let invalid = self.case_index(&error_type, "invalidFormat", origin)?;
                let range = self.case_index(&error_type, "outOfRange", origin)?;
                let ok = self.case_index(result, "ok", origin)?;
                let err = self.case_index(result, "err", origin)?;
                let held = self.temp();
                let held_at = self.cell(held);
                self.native_operands(&row, operands, classes);
                self.push(&[
                    get(FRAME),
                    c32(index(held_at)),
                    I::I32Add,
                    I::Call(import),
                    set(SCRATCH),
                ]);
                self.release_operands(operands, classes);
                self.set_byte(held, cell_class(class).code());
                let (good, bad, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.push(&[get(SCRATCH), I::I32Eqz]);
                self.end(super::Term::Branch {
                    then_block: good,
                    else_block: bad,
                });
                self.start(good);
                self.build(
                    Kind::Variant,
                    Variant::Fixed(ok),
                    &[Part::Slot { slot: held, class }],
                    dest,
                );
                self.goto(join, bad);
                self.push(&[
                    c32(invalid.cast_signed()),
                    c32(range.cast_signed()),
                    get(SCRATCH),
                    c32(1),
                    I::I32Eq,
                    I::Select,
                    set(SCRATCH),
                ]);
                let error = self.temp();
                self.build(
                    Kind::Variant,
                    Variant::Local {
                        local: SCRATCH,
                        bias: 0,
                    },
                    &[],
                    error,
                );
                self.build(
                    Kind::Variant,
                    Variant::Fixed(err),
                    &[Part::Slot {
                        slot: error,
                        class: ValueClass::Ref,
                    }],
                    dest,
                );
                self.goto(join, join);
            }
            Answer::Object { kind, totality } => {
                let kind = match kind {
                    ObjectKind::Str => Kind::Str,
                    ObjectKind::Bytes => Kind::Bytes,
                    ObjectKind::List => Kind::List,
                };
                // Measure: with no destination the import writes nothing and
                // answers the length, or `-1` when there is no object.
                self.native_operands(&row, operands, classes);
                self.push(&[c32(0), I::Call(import), set(WR)]);
                let object = self.temp();
                match totality {
                    Totality::Total => {
                        self.fill(&row, kind, operands, classes, import, object);
                        self.move_slot(dest, object);
                    }
                    Totality::Option | Totality::Conversion => {
                        let success = if totality == Totality::Option {
                            "some"
                        } else {
                            "ok"
                        };
                        let success = self.case_index(result, success, origin)?;
                        let (absent, present, join) =
                            (self.new_block(), self.new_block(), self.new_block());
                        self.push(&[get(WR), c64(0), I::I64LtS]);
                        self.end(super::Term::Branch {
                            then_block: absent,
                            else_block: present,
                        });
                        self.start(present);
                        self.fill(&row, kind, operands, classes, import, object);
                        self.build(
                            Kind::Variant,
                            Variant::Fixed(success),
                            &[Part::Slot {
                                slot: object,
                                class: ValueClass::Ref,
                            }],
                            dest,
                        );
                        self.goto(join, absent);
                        if totality == Totality::Option {
                            let none = self.case_index(result, "none", origin)?;
                            self.build(Kind::Variant, Variant::Fixed(none), &[], dest);
                        } else {
                            let error_type =
                                self.payload_type(result, "err", origin)?;
                            let invalid =
                                self.case_index(&error_type, "invalidFormat", origin)?;
                            let err = self.case_index(result, "err", origin)?;
                            let error = self.temp();
                            self.build(
                                Kind::Variant,
                                Variant::Fixed(invalid),
                                &[],
                                error,
                            );
                            self.build(
                                Kind::Variant,
                                Variant::Fixed(err),
                                &[Part::Slot {
                                    slot: error,
                                    class: ValueClass::Ref,
                                }],
                                dest,
                            );
                        }
                        self.goto(join, join);
                    }
                }
                self.release_operands(operands, classes);
            }
        }
        Ok(())
    }

    /// Allocates an object of `kind` to the length the import measured, in
    /// `WR`, and fills it.
    fn fill(
        &mut self,
        row: &NativeRow,
        kind: Kind,
        operands: &[u32],
        classes: &[ValueClass],
        import: u32,
        object: u32,
    ) {
        let new = self.lowering.fns.new;
        self.push(&[
            c32(kind.code().cast_signed()),
            c32(index(layout::row(kind).stride)),
            get(WR),
            I::I32WrapI64,
            c32(0),
            I::Call(new),
            set(OBJECT),
        ]);
        self.store_object(object);
        self.native_operands(row, operands, classes);
        self.load_ref(object);
        self.push(&[I::Call(import), I::Drop]);
    }

    /// Whether a value of the closed type `ty` has a canonical encoding that
    /// `assert.equal` compares: one with no function value in it.
    pub(super) fn observable(
        &self,
        ty: &Type,
        origin: &SourceOrigin,
    ) -> Result<bool, EmitError> {
        let mut seen = Vec::new();
        let mut pending = vec![ty.clone()];
        while let Some(ty) = pending.pop() {
            if seen.contains(&ty) {
                continue;
            }
            let shape = self.shape(&ty, origin)?;
            seen.push(ty);
            match shape {
                Shape::Function(_) => return Ok(false),
                Shape::Scalar(_) | Shape::Atom | Shape::Str | Shape::Bytes => {}
                Shape::Dict(key, value) => pending.extend([key, value]),
                Shape::Tuple(types) => pending.extend(types),
                Shape::List(element) | Shape::Wrapper(element) => pending.push(element),
                Shape::Record(fields) => {
                    pending.extend(fields.into_iter().map(|(_, ty)| ty))
                }
                Shape::Variant(cases) => pending.extend(
                    cases
                        .into_iter()
                        .flat_map(|(_, fields)| fields.into_iter().map(|(_, ty)| ty)),
                ),
            }
        }
        Ok(true)
    }

    /// Leaves on the operand stack whether the cells of two slots of `class`
    /// are canonically equal: by bits, except that every NaN of a float type
    /// is equal to every other, since every NaN observes as the canonical one.
    pub(super) fn scalar_equal(&mut self, left: u32, right: u32, class: ValueClass) {
        let (left, right) = (self.cell(left), self.cell(right));
        match class {
            ValueClass::F32 => {
                let nan = |at: u32| {
                    [
                        get(FRAME),
                        ld32(at),
                        c32(0x7FFF_FFFF),
                        I::I32And,
                        c32(0x7F80_0000),
                        I::I32GtU,
                    ]
                };
                self.push(&[get(FRAME), ld32(left), get(FRAME), ld32(right), I::I32Eq]);
                self.push(&nan(left));
                self.push(&nan(right));
                self.push(&[I::I32And, I::I32Or]);
            }
            ValueClass::F64 => {
                let nan = |at: u32| {
                    [
                        get(FRAME),
                        ld64(at),
                        c64(0x7FFF_FFFF_FFFF_FFFF),
                        I::I64And,
                        c64(0x7FF0_0000_0000_0000),
                        I::I64GtU,
                    ]
                };
                self.push(&[get(FRAME), ld64(left), get(FRAME), ld64(right), I::I64Eq]);
                self.push(&nan(left));
                self.push(&nan(right));
                self.push(&[I::I32And, I::I32Or]);
            }
            _ => {
                self.push(&[get(FRAME), ld64(left), get(FRAME), ld64(right), I::I64Eq])
            }
        }
    }
}
