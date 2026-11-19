//! Calls, function values, and closures.
//!
//! A call evaluates its callee, then its positional operands and its labelled
//! ones in written order, and moves each into the callee's slot: the
//! positional ones first and then the labelled ones in name order, which is
//! how the checked IR numbers a callable's slots. A label the call does not
//! write takes the callee's declared default. A call of a function value also
//! moves the value into the callee's environment slot, where a closure's body
//! reads its captures. A tail call replaces the activation, so a loop written
//! as tail recursion runs in a bounded frame stack.
//!
//! A function value is an object of the `function` kind whose cells name the
//! instance, its frame size, and its arity, followed by the captures of a
//! closure.

use vibra_ir::{Arguments, CallTarget, ClosureBody, Expr, Node, SourceOrigin, Type};

use super::{Builder, Callable, Callee, FRAME, Move, Part, Term, Variant};
use crate::error::EmitError;
use crate::layout::{Kind, frame, state};
use crate::runtime::{ValueClass, c32, get, ld32, ld64, st8, st64};

impl<'a> Builder<'_, 'a> {
    /// Stores the instance number, frame size, and arity of a function value
    /// in new temporaries: the first parts of its object.
    fn function_cells(
        &mut self,
        instance: u32,
        arity: usize,
    ) -> Result<Vec<Part>, EmitError> {
        let arity = u32::try_from(arity).map_err(|_| EmitError::ModuleSize)?;
        let slots = self.lowering.slots_of(instance);
        let mut parts = Vec::with_capacity(3);
        // The cells in the order of `fv`: the instance, the frame size, and
        // the arity.
        for bits in [instance, slots, arity] {
            let slot = self.temp();
            self.store_bits(slot, u64::from(bits), ValueClass::I32);
            parts.push(Part::Slot {
                slot,
                class: ValueClass::I32,
            });
        }
        Ok(parts)
    }

    /// The value of the function `function` at `type_arguments`.
    pub(super) fn function_value(
        &mut self,
        function: usize,
        type_arguments: &[Type],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let arguments = type_arguments
            .iter()
            .map(|ty| self.close(ty))
            .collect::<Vec<_>>();
        let ty = self
            .lowering
            .types
            .function_type(function, &arguments, origin)?;
        let arity = match &ty {
            Type::Fn(signature) => signature.slot_count(),
            Type::Proc(_) => return Err(EmitError::unsupported("proc", origin)),
            other => {
                return Err(EmitError::malformed(
                    format!("a function of type {other}"),
                    origin,
                ));
            }
        };
        if self
            .lowering
            .set
            .functions()
            .get(function)
            .is_some_and(|function| !function.defaults.is_empty())
        {
            // A call through the value would need the defaults, which a value
            // does not carry in this profile.
            return Err(EmitError::unsupported("default", origin));
        }
        let instance =
            self.lowering
                .instance(Callable::Function(function), arguments, origin)?;
        let mark = self.next_temp;
        let parts = self.function_cells(instance, arity)?;
        self.build(Kind::Function, Variant::Fixed(0), &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// The value of a closure: its instance at the enclosing type arguments,
    /// and its captures, evaluated in order.
    pub(super) fn closure_value(
        &mut self,
        closure: &'a ClosureBody,
        captures: &'a [Node],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let arity = match self.close(&closure.value_type) {
            Type::Fn(signature) => signature.slot_count(),
            Type::Proc(_) => return Err(EmitError::unsupported("proc", origin)),
            other => {
                return Err(EmitError::malformed(
                    format!("a closure of type {other}"),
                    origin,
                ));
            }
        };
        if captures.len() != closure.captures.len() {
            return Err(EmitError::malformed("a closure's captures", origin));
        }
        let arguments = self.arguments.clone();
        let instance =
            self.lowering
                .instance(Callable::Closure(closure), arguments, origin)?;
        let mark = self.next_temp;
        let mut parts = self.function_cells(instance, arity)?;
        for (capture, ty) in captures.iter().zip(&closure.captures) {
            let class = self.class(&self.close(ty), origin)?;
            let slot = self.temp();
            self.expr(capture, slot)?;
            parts.push(Part::Slot { slot, class });
        }
        self.build(Kind::Function, Variant::Fixed(0), &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// A call, or a tail call, of `target`, whose result goes to `dest`.
    pub(super) fn call(
        &mut self,
        target: &'a CallTarget,
        arguments: &'a Arguments,
        tail: bool,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let mark = self.next_temp;
        let (callee, defaults) = match target {
            CallTarget::Direct {
                function,
                type_arguments,
            } => {
                let closed = type_arguments
                    .iter()
                    .map(|ty| self.close(ty))
                    .collect::<Vec<_>>();
                let instance = self.lowering.instance(
                    Callable::Function(*function),
                    closed,
                    origin,
                )?;
                let slots = self.lowering.slots_of(instance);
                let defaults = self
                    .lowering
                    .set
                    .functions()
                    .get(*function)
                    .map(|function| function.defaults.as_slice());
                (
                    Callee::Named {
                        function: instance,
                        slots,
                    },
                    defaults,
                )
            }
            CallTarget::Value(callee) => {
                let slot = self.temp();
                self.expr(callee, slot)?;
                (Callee::Value { slot }, None)
            }
            CallTarget::Contract(_) => {
                return Err(EmitError::unsupported("contractCall", origin));
            }
        };
        let arity = match self.close(&self.lowering.types.callee_type(target, origin)?)
        {
            Type::Fn(signature) => signature.slot_count(),
            Type::Proc(_) => return Err(EmitError::unsupported("proc", origin)),
            other => {
                return Err(EmitError::malformed(format!("a call of {other}"), origin));
            }
        };
        let mut moves = Vec::with_capacity(arity + 1);
        for (position, argument) in (0_u32..).zip(&arguments.positional) {
            let slot = self.temp();
            self.expr(argument, slot)?;
            moves.push(Move {
                from: slot,
                to: position,
            });
        }
        let base = u32::try_from(arguments.positional.len())
            .map_err(|_| EmitError::ModuleSize)?;
        let mut names = arguments
            .labelled
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        for (name, argument) in &arguments.labelled {
            let rank = names
                .iter()
                .position(|known| *known == name)
                .and_then(|rank| u32::try_from(rank).ok())
                .ok_or(EmitError::ModuleSize)?;
            let slot = self.temp();
            if let Expr::Default(ty) = &argument.expr {
                let constant = defaults
                    .and_then(|defaults| {
                        defaults.iter().find(|(label, _)| label == name)
                    })
                    .map(|(_, constant)| constant)
                    .ok_or_else(|| {
                        EmitError::unsupported("default", &argument.origin)
                    })?;
                let ty = self.close(ty);
                self.constant(constant.value(), &ty, &argument.origin, slot)?;
            } else {
                self.expr(argument, slot)?;
            }
            moves.push(Move {
                from: slot,
                to: base + rank,
            });
        }
        let arity = u32::try_from(arity).map_err(|_| EmitError::ModuleSize)?;
        if u32::try_from(moves.len()).ok() != Some(arity) {
            return Err(EmitError::malformed("a call's operand count", origin));
        }
        if let Callee::Value { slot } = callee {
            moves.push(Move {
                from: slot,
                to: arity,
            });
        }
        if tail {
            self.stage = self.stage.max(moves.len());
            self.end(Term::Tail {
                callee,
                moves,
                arity,
            });
            self.dead();
        } else {
            let resume = self.new_block();
            self.end(Term::Call {
                callee,
                moves,
                arity,
                resume,
            });
            self.start(resume);
            // The callee handed its result over in `ret`, with its class byte.
            let at = self.cell(dest);
            self.push(&[
                get(FRAME),
                c32(0),
                ld64(state::RET),
                st64(at),
                get(FRAME),
                c32(0),
                ld32(state::RET_CLASS),
                st8(frame::class_offset(dest)),
            ]);
        }
        self.release_to(mark);
        Ok(())
    }
}
