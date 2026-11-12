//! The text form of a [`Program`]: what a test failure or a snapshot shows.
//!
//! The form is deterministic and complete. It prints every field the types
//! hold, in the order the types hold them, and it reads nothing but the
//! program: no address, no hash order, no clock. Two programs that differ in
//! any field print different text. Strings are printed with Rust's debug
//! escaping, so a name or constant cannot be confused with the layout around
//! it.
//!
//! ```text
//! program entry=f0 globals=[f1]
//!   constants
//!     c0 str "hi"
//!   origins
//!     o1 "a.vib" 0..4
//! fn f0 "main" params=0 env=false slots=2
//!   b0:
//!     s0 = scalar i64 7
//!     s1 = const c0
//!     return s1
//! ```
//!
//! Slots print as `s<n>`, blocks as `b<n>`, functions as `f<n>`, constants as
//! `c<n>`, module values as `g<n>`, and origin ordinals as `o<n>`, where `o0`
//! is "no origin". An indirect callee prints as `*s<n>`.

use std::fmt::{self, Display, Formatter};

use crate::{Callee, Constant, Function, Inst, OriginId, Program, Slot, Terminator};

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "program entry=f{} globals=", self.entry.0)?;
        list(f, self.globals.iter().map(|id| format!("f{}", id.0)))?;
        writeln!(f)?;
        writeln!(f, "  constants")?;
        for (index, constant) in self.constants.iter().enumerate() {
            match constant {
                Constant::Str(text) => writeln!(f, "    c{index} str {text:?}")?,
                Constant::Bytes(bytes) => writeln!(f, "    c{index} bytes {bytes:?}")?,
                Constant::Atom(name) => writeln!(f, "    c{index} atom {name:?}")?,
            }
        }
        writeln!(f, "  origins")?;
        for (index, origin) in self.origins.iter().enumerate() {
            writeln!(
                f,
                "    o{} {:?} {:?}",
                index.saturating_add(1),
                origin.source_id(),
                origin.span()
            )?;
        }
        for (index, function) in self.functions.iter().enumerate() {
            function_text(f, index, function)?;
        }
        Ok(())
    }
}

fn function_text(
    f: &mut Formatter<'_>,
    index: usize,
    function: &Function,
) -> fmt::Result {
    writeln!(
        f,
        "fn f{index} {:?} params={} env={} slots={}",
        function.name, function.params, function.env, function.slots
    )?;
    for (block_index, block) in function.blocks.iter().enumerate() {
        writeln!(f, "  b{block_index}:")?;
        for inst in &block.insts {
            write!(f, "    ")?;
            inst_text(f, inst)?;
            writeln!(f)?;
        }
        write!(f, "    ")?;
        term_text(f, &block.term)?;
        writeln!(f)?;
    }
    Ok(())
}

fn inst_text(f: &mut Formatter<'_>, inst: &Inst) -> fmt::Result {
    match inst {
        Inst::Scalar { dst, class, bits } => {
            write!(f, "{} = scalar {class:?} {bits}", slot(*dst))
        }
        Inst::Constant { dst, constant } => {
            write!(f, "{} = const c{}", slot(*dst), constant.0)
        }
        Inst::Copy { dst, src } => write!(f, "{} = copy {}", slot(*dst), slot(*src)),
        Inst::Move { dst, src } => write!(f, "{} = move {}", slot(*dst), slot(*src)),
        Inst::Drop { slot: dropped } => write!(f, "drop {}", slot(*dropped)),
        Inst::Op { dst, op, args } => {
            write!(f, "{} = op {op:?} ", slot(*dst))?;
            slots(f, args)
        }
        Inst::Intrinsic {
            dst,
            intrinsic,
            args,
            origin,
        } => {
            write!(f, "{} = intrinsic {:?} ", slot(*dst), intrinsic.symbol())?;
            slots(f, args)?;
            write!(f, " {}", origin_text(*origin))
        }
        Inst::Build {
            dst,
            kind,
            variant,
            cells,
        } => {
            write!(f, "{} = build {kind:?} variant={variant} ", slot(*dst))?;
            slots(f, cells)
        }
        Inst::Cell { dst, object, index } => {
            write!(f, "{} = cell {} .{index}", slot(*dst), slot(*object))
        }
        Inst::Variant { dst, object } => {
            write!(f, "{} = variant {}", slot(*dst), slot(*object))
        }
        Inst::Function {
            dst,
            function,
            captures,
        } => {
            write!(f, "{} = function f{} ", slot(*dst), function.0)?;
            slots(f, captures)
        }
    }
}

fn term_text(f: &mut Formatter<'_>, term: &Terminator) -> fmt::Result {
    match term {
        Terminator::Jump(target) => write!(f, "jump b{}", target.0),
        Terminator::Branch {
            cond,
            then_block,
            else_block,
        } => write!(
            f,
            "branch {} then b{} else b{}",
            slot(*cond),
            then_block.0,
            else_block.0
        ),
        Terminator::Switch {
            value,
            targets,
            default,
        } => {
            write!(f, "switch {} ", slot(*value))?;
            list(f, targets.iter().map(|target| format!("b{}", target.0)))?;
            write!(f, " default b{}", default.0)
        }
        Terminator::Call {
            callee,
            args,
            dst,
            resume,
        } => {
            write!(f, "{} = call {} ", slot(*dst), callee_text(*callee))?;
            slots(f, args)?;
            write!(f, " -> b{}", resume.0)
        }
        Terminator::TailCall { callee, args } => {
            write!(f, "tailcall {} ", callee_text(*callee))?;
            slots(f, args)
        }
        Terminator::Host {
            row,
            args,
            dst,
            resume,
            origin,
        } => {
            write!(f, "{} = host {:?} ", slot(*dst), row.symbol)?;
            slots(f, args)?;
            write!(f, " -> b{} {}", resume.0, origin_text(*origin))
        }
        Terminator::Global {
            global,
            dst,
            resume,
        } => write!(f, "{} = global g{} -> b{}", slot(*dst), global.0, resume.0),
        Terminator::Return { value } => write!(f, "return {}", slot(*value)),
        Terminator::Trap { code, origin } => {
            write!(f, "trap {code:?} {}", origin_text(*origin))
        }
    }
}

fn slot(slot: Slot) -> String {
    format!("s{}", slot.0)
}

fn origin_text(origin: OriginId) -> String {
    format!("o{}", origin.0)
}

fn callee_text(callee: Callee) -> String {
    match callee {
        Callee::Direct(id) => format!("f{}", id.0),
        Callee::Indirect(held) => format!("*{}", slot(held)),
    }
}

fn slots(f: &mut Formatter<'_>, items: &[Slot]) -> fmt::Result {
    list(f, items.iter().map(|item| slot(*item)))
}

fn list(f: &mut Formatter<'_>, items: impl Iterator<Item = String>) -> fmt::Result {
    write!(f, "[")?;
    for (position, item) in items.enumerate() {
        if position > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{item}")?;
    }
    write!(f, "]")
}
