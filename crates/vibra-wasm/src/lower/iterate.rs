//! The closed `iter.next` of the builtin constructor types.
//!
//! `(array t)`, `(dict k v)`, `str`, and `(option t)` conform to `iter` without a
//! written implementation (`docs/spec/02-type-system.md`, "Closed builtin
//! conformance"), so a call of `iter.next` that no written implementation
//! describes is answered here, from the descriptor of the type the iterator
//! has. `next` yields the first item and the iterator that remains: an array
//! its first element and the rest of its elements, a dict its first entry, a
//! pair of the key and the value, and a dict of the entries after it, a `str`
//! its first scalar and the scalars after it, and an `option` its payload and
//! `none`. The remainder is built by the registry's own slice, so a count is
//! taken for each element it holds. Where the call expects the remainder as an
//! interface value, it is widened with the descriptor of the iterator's type.

use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{SourceOrigin, Type};
use wasm_encoder::Instruction as I;

use super::dispatch::is_interface;
use super::{Builder, FRAME, Kind, Part, Term};
use crate::desc::class;
use crate::form::NotLowered;
use crate::layout::{CellClass, header};
use crate::runtime::{ValueClass, c32, get, ld32, st8, st64};

impl Builder<'_, '_> {
    /// Answers `iter.next` of the builtin iterator in `operands`, whose type
    /// the descriptor in `recv_desc` says, into `dest`.
    #[allow(clippy::too_many_lines)]
    pub(super) fn closed_next(
        &mut self,
        operands: &[u32],
        recv_desc: u32,
        argument_descs: &[u32],
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let unsupported = || NotLowered::type_of("shape", Some(origin.clone()));
        let [iterator] = operands else {
            return Err(unsupported());
        };
        let iterator = *iterator;
        for slot in argument_descs {
            self.discard(*slot, ValueClass::Dyn);
        }
        // The answer is an `option` of the item and the iterator that remains.
        let Type::Applied(option, arguments) = result else {
            return Err(unsupported());
        };
        let [Type::Tuple(pair)] = arguments.as_slice() else {
            return Err(unsupported());
        };
        let [item_type, rest_type] = pair.as_slice() else {
            return Err(unsupported());
        };
        let widen = is_interface(rest_type);
        let some = self.discriminant(result, "some", origin)?;
        let none = self.discriminant(result, "none", origin)?;
        let item_class = self.class(item_type, origin)?;
        let join = self.new_block();

        // `(array t)`, and `(dict k v)`, whose first element is its first entry.
        for (kind, is_dict) in [(class::ARRAY, false), (class::DICT, true)] {
            let (form, next) = (self.new_block(), self.new_block());
            self.push_class_is(recv_desc, kind);
            self.end(Term::Branch {
                then_block: form,
                else_block: next,
            });
            self.start(form);
            self.next_of_cells(
                iterator,
                recv_desc,
                is_dict,
                widen,
                (some, none),
                item_class,
                option,
                origin,
                dest,
            )?;
            self.goto(join, next);
        }

        // `str`.
        let (form, next) = (self.new_block(), self.new_block());
        self.push_class_is(recv_desc, class::TEXT);
        self.end(Term::Branch {
            then_block: form,
            else_block: next,
        });
        self.start(form);
        self.next_of_text(
            iterator,
            recv_desc,
            widen,
            (some, none),
            item_class,
            option,
            result,
            origin,
            dest,
        )?;
        self.goto(join, next);

        // `(option t)`.
        let (form, next) = (self.new_block(), self.new_block());
        let head = self
            .lowering
            .head(&Type::Applied(option.clone(), vec![Type::Void]));
        self.push_head(recv_desc);
        self.push(&[c32(head.cast_signed()), I::I32Eq]);
        self.end(Term::Branch {
            then_block: form,
            else_block: next,
        });
        self.start(form);
        self.next_of_option(
            iterator,
            recv_desc,
            widen,
            (some, none),
            item_type,
            result,
            origin,
            dest,
        )?;
        self.goto(join, next);

        // Another type has no closed conformance, which the checker rejects.
        let stop = self.lowering.fns.stop_trap;
        self.push(&[
            c32(vibra_ir::boundary::TrapCode::InvalidCheckedProgram.code()),
            I::Call(stop),
        ]);
        self.goto(join, join);
        Ok(())
    }

    /// The remainder `rest`, widened with the iterator's own descriptor when the
    /// call expects an interface value.
    fn remainder(&mut self, rest: u32, recv_desc: u32, widen: bool) -> u32 {
        if !widen {
            return rest;
        }
        let (desc, wrapped) = (self.temp(), self.temp());
        self.copy_cell(desc, recv_desc);
        self.claim(desc, ValueClass::Dyn, super::ByteSource::Slot(recv_desc));
        self.wrap_pair(desc, rest, wrapped);
        wrapped
    }

    /// Builds `some` of the pair of `first` and `rest`.
    fn some_pair(
        &mut self,
        first: u32,
        item_class: ValueClass,
        rest: u32,
        some: u32,
        dest: u32,
    ) {
        let pair = self.temp();
        self.build(
            Kind::Tuple,
            0,
            &[
                Part::Slot {
                    slot: first,
                    class: item_class,
                },
                Part::Slot {
                    slot: rest,
                    class: ValueClass::Ref,
                },
            ],
            pair,
        );
        self.build(
            Kind::Enum,
            some,
            &[Part::Slot {
                slot: pair,
                class: ValueClass::Ref,
            }],
            dest,
        );
    }

    /// Stores the length of the object in `object` in the 64-bit slot `slot`.
    fn store_length(&mut self, slot: u32, object: u32) {
        let (at, held) = (self.cell(slot), self.cell(object));
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld32(held),
            ld32(header::LEN),
            I::I64ExtendI32U,
            st64(at),
        ]);
        self.set_byte(slot, CellClass::I64.code());
    }

    /// The first element of an array or the first entry of a dict, and the
    /// elements after it.
    #[allow(clippy::too_many_arguments)]
    fn next_of_cells(
        &mut self,
        iterator: u32,
        recv_desc: u32,
        is_dict: bool,
        widen: bool,
        (some, none): (u32, u32),
        item_class: ValueClass,
        option: &vibra_ir::TypeId,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let held = self.cell(iterator);
        self.push(&[get(FRAME), ld32(held), ld32(header::LEN)]);
        let (empty, present) = (self.new_block(), self.new_block());
        let done = self.new_block();
        self.end(Term::Branch {
            then_block: present,
            else_block: empty,
        });
        self.start(empty);
        self.drop_slot(iterator);
        self.build(Kind::Enum, none, &[], dest);
        self.goto(done, present);

        let first = self.temp();
        let zero = self.temp();
        self.store_word(zero, &[c32(0)]);
        self.load_indexed(first, iterator, zero);
        let (one, length, slice) = (self.temp(), self.temp(), self.temp());
        self.store_bits(one, 1, ValueClass::I64);
        self.store_length(length, iterator);
        let row = CompilerIntrinsic::ArraySlice
            .native_import()
            .ok_or_else(|| NotLowered::type_of("shape", Some(origin.clone())))?;
        let sliced =
            Type::Applied(option.clone(), vec![Type::Array(Box::new(Type::Void))]);
        self.native_on(
            row,
            &[iterator, one, length],
            &[ValueClass::Ref, ValueClass::I64, ValueClass::I64],
            &sliced,
            origin,
            false,
            slice,
        )?;
        let rest = self.temp();
        self.copy_part(rest, slice, 0, 1, ValueClass::Ref);
        self.drop_slot(slice);
        if is_dict {
            // The entries after the first are a dict: sorted, with distinct keys.
            let at = self.cell(rest);
            self.push(&[
                get(FRAME),
                ld32(at),
                c32(Kind::Dict.code().cast_signed()),
                st8(header::KIND),
            ]);
        }
        self.drop_slot(iterator);
        let rest = self.remainder(rest, recv_desc, widen);
        let item_class = if is_dict { ValueClass::Ref } else { item_class };
        self.some_pair(first, item_class, rest, some, dest);
        self.goto(done, done);
        self.release_to(mark);
        Ok(())
    }

    /// The first scalar of a `str`, and the scalars after it.
    #[allow(clippy::too_many_arguments)]
    fn next_of_text(
        &mut self,
        iterator: u32,
        recv_desc: u32,
        widen: bool,
        (some, none): (u32, u32),
        item_class: ValueClass,
        option: &vibra_ir::TypeId,
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let _ = result;
        let mark = self.next_temp;
        let held = self.cell(iterator);
        self.push(&[get(FRAME), ld32(held), ld32(header::LEN)]);
        let (empty, present) = (self.new_block(), self.new_block());
        let done = self.new_block();
        self.end(Term::Branch {
            then_block: present,
            else_block: empty,
        });
        self.start(empty);
        self.drop_slot(iterator);
        self.build(Kind::Enum, none, &[], dest);
        self.goto(done, present);

        let first = self.temp();
        let at = self.cell(first);
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld32(held),
            ld32(header::SIZE),
            I::I64ExtendI32U,
            st64(at),
        ]);
        self.set_byte(first, CellClass::I32.code());
        let (one, length, slice) = (self.temp(), self.temp(), self.temp());
        self.store_bits(one, 1, ValueClass::I64);
        self.store_length(length, iterator);
        let row = CompilerIntrinsic::TextSlice
            .native_import()
            .ok_or_else(|| NotLowered::type_of("shape", Some(origin.clone())))?;
        let sliced = Type::Applied(option.clone(), vec![Type::Str]);
        self.native_on(
            row,
            &[iterator, one, length],
            &[ValueClass::Ref, ValueClass::I64, ValueClass::I64],
            &sliced,
            origin,
            false,
            slice,
        )?;
        let rest = self.temp();
        self.copy_part(rest, slice, 0, 1, ValueClass::Ref);
        self.drop_slot(slice);
        self.drop_slot(iterator);
        let rest = self.remainder(rest, recv_desc, widen);
        self.some_pair(first, item_class, rest, some, dest);
        self.goto(done, done);
        self.release_to(mark);
        Ok(())
    }

    /// The payload of an `option` once, and `none` after it.
    #[allow(clippy::too_many_arguments)]
    fn next_of_option(
        &mut self,
        iterator: u32,
        recv_desc: u32,
        widen: bool,
        (some, none): (u32, u32),
        item_type: &Type,
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let _ = result;
        let mark = self.next_temp;
        // The iterator is an `option` of the item type, whose `some` carries the
        // payload.
        let held = self.cell(iterator);
        let option_type = Type::Applied(
            match result {
                Type::Applied(option, _) => option.clone(),
                _ => return Err(NotLowered::type_of("shape", Some(origin.clone()))),
            },
            vec![item_type.clone()],
        );
        let present_variant = self.discriminant(&option_type, "some", origin)?;
        let absent_variant = self.discriminant(&option_type, "none", origin)?;
        self.push(&[
            get(FRAME),
            ld32(held),
            ld32(header::VARIANT),
            c32(present_variant.cast_signed()),
            I::I32Eq,
        ]);
        let (present, empty, done) =
            (self.new_block(), self.new_block(), self.new_block());
        self.end(Term::Branch {
            then_block: present,
            else_block: empty,
        });
        self.start(empty);
        self.drop_slot(iterator);
        self.build(Kind::Enum, none, &[], dest);
        self.goto(done, present);

        let class = self.class(item_type, origin)?;
        let first = self.temp();
        match item_type {
            // A `some` of `void` has no payload cell, and yields the item
            // `void`, which is the scalar `0` wherever a slot exists.
            Type::Void => self.store_bits(first, 0, ValueClass::Void),
            // A payload of a generic type is a cell, or none when the type
            // argument is `void`: the object's length says which.
            Type::Param(_) => {
                let (cell, bare, joined) =
                    (self.new_block(), self.new_block(), self.new_block());
                let held = self.cell(iterator);
                self.push(&[get(FRAME), ld32(held), ld32(header::LEN)]);
                self.end(Term::Branch {
                    then_block: cell,
                    else_block: bare,
                });
                self.start(bare);
                self.store_bits(first, 0, ValueClass::Void);
                self.goto(joined, cell);
                self.copy_part(first, iterator, 0, 1, class);
                self.goto(joined, joined);
            }
            _ => self.copy_part(first, iterator, 0, 1, class),
        }
        self.drop_slot(iterator);
        let rest = self.temp();
        self.build(Kind::Enum, absent_variant, &[], rest);
        let rest = self.remainder(rest, recv_desc, widen);
        self.some_pair(first, class, rest, some, dest);
        self.goto(done, done);
        self.release_to(mark);
        Ok(())
    }
}
