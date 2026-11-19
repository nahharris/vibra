//! Patterns: the test of a subject against a pattern, and its bindings.
//!
//! A pattern reads the subject's components into temporaries that borrow
//! them (their class bytes say they own nothing), so a refused pattern leaves
//! no count to undo, and only a binding takes a count of its own. A binding
//! writes a slot of the checked IR, which may hold an earlier value, so that
//! value is dropped first.

use vibra_ir::{Pattern, SourceOrigin, Type};
use wasm_encoder::Instruction as I;

use super::{Builder, FRAME, SCRATCH, Shape};
use crate::error::EmitError;
use crate::layout::header;
use crate::runtime::{ValueClass, c32, c64, get, ld32, ld64, set};
use crate::types::field_position;

impl<'a> Builder<'_, 'a> {
    /// Tests the value in `subject`, of the closed type `ty`, against
    /// `pattern`, binding its slots, and continues at `fail` when it does not
    /// match. The subject is not consumed.
    pub(super) fn pattern(
        &mut self,
        pattern: &Pattern,
        subject: u32,
        ty: &Type,
        fail: u32,
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        match pattern {
            Pattern::Discard => Ok(()),
            Pattern::Bind(slot) => {
                let class = self.class(ty, origin)?;
                let to = self.ir_slot(*slot)?;
                self.drop_if_owned(to);
                self.copy_cell(to, subject);
                self.claim(to, class);
                Ok(())
            }
            Pattern::Literal(constant) => {
                let constant_type = self.close(constant.value_type());
                match self.class(&constant_type, origin)? {
                    ValueClass::Ref => {
                        let mark = self.next_temp;
                        let held = self.temp();
                        self.constant(constant.value(), &constant_type, origin, held)?;
                        let deep_equal = self.lowering.deep_equal();
                        self.load_ref(subject);
                        self.load_ref(held);
                        self.push(&[I::Call(deep_equal), set(SCRATCH)]);
                        self.drop_slot(held);
                        self.push(&[get(SCRATCH)]);
                        self.release_to(mark);
                    }
                    _ => {
                        let bits =
                            super::scalar_bits(constant.value()).ok_or_else(|| {
                                EmitError::malformed("a constant pattern", origin)
                            })?;
                        let at = self.cell(subject);
                        self.push(&[
                            get(FRAME),
                            ld64(at),
                            c64(bits.cast_signed()),
                            I::I64Eq,
                        ]);
                    }
                }
                self.continue_unless(fail);
                Ok(())
            }
            Pattern::Tuple(items) => {
                let Shape::Tuple(types) = self.shape(ty, origin)? else {
                    return Err(EmitError::malformed(
                        format!("a tuple pattern on {ty}"),
                        origin,
                    ));
                };
                if types.len() != items.len() {
                    return Err(EmitError::malformed(
                        "a tuple pattern's arity",
                        origin,
                    ));
                }
                let parts = items.iter().zip(types).collect::<Vec<_>>();
                self.components(&parts, subject, fail, origin)
            }
            Pattern::Record(written) => {
                let Shape::Record(fields) = self.shape(ty, origin)? else {
                    return Err(EmitError::malformed(
                        format!("a record pattern on {ty}"),
                        origin,
                    ));
                };
                self.fields_of(written, &fields, subject, fail, origin)
            }
            Pattern::List(items) => {
                let Shape::List(element) = self.shape(ty, origin)? else {
                    return Err(EmitError::malformed(
                        format!("a list pattern on {ty}"),
                        origin,
                    ));
                };
                let len =
                    u32::try_from(items.len()).map_err(|_| EmitError::ModuleSize)?;
                self.load_ref(subject);
                self.push(&[ld32(header::LEN), c32(len.cast_signed()), I::I32Eq]);
                self.continue_unless(fail);
                let parts = items
                    .iter()
                    .map(|item| (item, element.clone()))
                    .collect::<Vec<_>>();
                self.components(&parts, subject, fail, origin)
            }
            Pattern::Wrapper(inner) => match self.shape(ty, origin)? {
                Shape::Wrapper(representation) => self.components(
                    &[(inner.as_ref(), representation)],
                    subject,
                    fail,
                    origin,
                ),
                _ => Err(EmitError::unsupported("wrapperPattern", origin)),
            },
            Pattern::Case { case, fields } => {
                let shape = self.shape(ty, origin)?;
                let (position, layout) = shape.case(case).ok_or_else(|| {
                    EmitError::malformed(format!("{ty} has no case `{case}`"), origin)
                })?;
                let layout = layout.to_vec();
                self.is_case(subject, position);
                self.continue_unless(fail);
                self.fields_of(fields, &layout, subject, fail, origin)
            }
        }
    }

    /// Tests the components of the object in `subject`, all of them, in
    /// order.
    fn components(
        &mut self,
        parts: &[(&Pattern, Type)],
        subject: u32,
        fail: u32,
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        let len = u32::try_from(parts.len()).map_err(|_| EmitError::ModuleSize)?;
        for (position, (pattern, ty)) in (0_u32..).zip(parts) {
            self.component(pattern, subject, position, len, ty, fail, origin)?;
        }
        Ok(())
    }

    /// Tests the fields `written` of the object in `subject`, whose fields are
    /// `fields` in layout order.
    fn fields_of(
        &mut self,
        written: &[(String, Pattern)],
        fields: &[(String, Type)],
        subject: u32,
        fail: u32,
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        let len = u32::try_from(fields.len()).map_err(|_| EmitError::ModuleSize)?;
        for (name, pattern) in written {
            let position = field_position(fields, name).ok_or_else(|| {
                EmitError::malformed(format!("no field `{name}`"), origin)
            })?;
            let ty = fields
                .get(position as usize)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| {
                    EmitError::malformed(format!("no field `{name}`"), origin)
                })?;
            self.component(pattern, subject, position, len, &ty, fail, origin)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn component(
        &mut self,
        pattern: &Pattern,
        subject: u32,
        position: u32,
        len: u32,
        ty: &Type,
        fail: u32,
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        if matches!(pattern, Pattern::Discard) {
            return Ok(());
        }
        let mark = self.next_temp;
        let part = self.temp();
        self.borrow_component(subject, position, len, part);
        self.pattern(pattern, part, ty, fail, origin)?;
        self.release_to(mark);
        Ok(())
    }
}
