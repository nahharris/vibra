//! Deterministic WebAssembly emission for checked Vibra IR.
//!
//! This is the second consumer of [`vibra_ir::CheckedProgram`], beside the
//! reference interpreter: it has no frontend of its own and lowers the same
//! typed IR the interpreter runs. It depends on `vibra-ir` and the encoder
//! only. It reaches no engine, and it does not depend on the native-code
//! crate: a module names a native only by an import from the pure module whose
//! name `vibra_ir::boundary` records.
//!
//! # Status
//!
//! Milestone 4 Step 5a built the memory layer of every module: the value arena,
//! precise reference counting with a worklist release, the handle table, the
//! instance state, and the host accessors ([`layout`] states the
//! representation). Step 5b lowers the first checked source programs: the
//! literals, sequences, `let`, `if`, and `return`; module values, evaluated
//! lazily and once; functions with fixed and labelled parameters and the direct
//! calls between them; and the nominal and structural data kinds with their
//! constructors, projections, and the injection of a member into a union.
//! Activations live in the arena and a call does not nest a WebAssembly call
//! ([`layout`], "Activations and the dispatcher"). Step 6 lowers calls of every
//! kind: function values and closures, a call through one, a tail call to every
//! kind of callee (the callee replaces the current frame), omitted labelled
//! operands, and generic functions and types, whose type arguments are passed to
//! each activation at run time. Step 7 lowers patterns and typed failure:
//! `match` with every pattern kind, destructuring in `let`, parameters, and
//! lambdas, `let-else`, `as` narrowing, `try`, and `never`, by one recursive
//! scheme over a table of what each kind of pattern asks of a value, so the
//! array pattern is the one pattern form left. Every other form returns
//! [`NotLowered`], naming each form, and never a module that omits part of the
//! program. Steps 8 onward move forms out of [`NotLowered`] one step at a time.
//!
//! # The module
//!
//! A module defines one 32-bit linear memory exported as `vibra_v1_memory`, one
//! function table holding its language functions, imports nothing (no program
//! lowered so far uses a native), exports the accessors of
//! `docs/spec/06-runtime.md`, "WebAssembly boundary", that do not depend on a
//! test, and has no custom section. Emission is deterministic: the bytes depend
//! only on the checked program.

mod classify;
mod encode;
mod form;
pub mod layout;
mod lower;
mod pattern;
mod runtime;
pub mod support;
mod types;

use vibra_ir::{CheckedProgram, SourceOrigin};

pub use form::{Form, NotLowered, UnloweredForm};

/// The origin table the toolchain produces beside a module.
///
/// A module carries no source map before Milestone 7, so a trap or a failed
/// assertion names its origin by an ordinal into this table, which maps each
/// ordinal to one source span (`docs/spec/06-runtime.md`, "Traps"). Ordinal `0`
/// means no origin, so the first entry has ordinal `1`. The table is not part
/// of the module's bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginTable {
    origins: Vec<SourceOrigin>,
}

impl OriginTable {
    /// The number of origins the table maps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.origins.len()
    }

    /// Whether the table maps no origin. No module lowered so far traps or
    /// asserts, so theirs is always empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty()
    }

    /// The origin an ordinal names, or `None` for ordinal `0` and for an
    /// ordinal outside the table.
    #[must_use]
    pub fn origin(&self, ordinal: u32) -> Option<&SourceOrigin> {
        let index = usize::try_from(ordinal).ok()?.checked_sub(1)?;
        self.origins.get(index)
    }
}

/// A module and the table that maps its origin ordinals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedModule {
    bytes: Vec<u8>,
    origins: OriginTable,
}

impl EmittedModule {
    /// The module's binary encoding.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The origin table produced beside the module.
    #[must_use]
    pub const fn origins(&self) -> &OriginTable {
        &self.origins
    }

    /// The module's binary encoding, by value.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Lowers a checked program to a v1 module.
///
/// The same program always yields the same bytes.
///
/// # Errors
///
/// [`NotLowered`] names every form the program uses that this step does not
/// lower. No module is produced for such a program.
pub fn emit(program: &CheckedProgram) -> Result<EmittedModule, NotLowered> {
    // A module indexes its functions with 32 bits. A count that does not fit is
    // reported, never wrapped.
    let counts = u32::try_from(program.functions().len())
        .ok()
        .zip(u32::try_from(program.globals().len()).ok())
        .and_then(|(functions, globals)| {
            Some((functions, globals, functions.checked_add(globals)?))
        });
    let entry = u32::try_from(program.entry_index()).ok();
    let Some(((_, globals, total), entry)) = counts.zip(entry) else {
        return Err(NotLowered::single(Form::ModuleSize));
    };
    if let Some(not_lowered) = NotLowered::from_uses(classify::unlowered(program)) {
        return Err(not_lowered);
    }
    let entry_function = program.entry();
    if !entry_function.signature().slot_types().is_empty() {
        return Err(NotLowered::from_uses(vec![
            form::UnloweredForm::new(
                Form::Parameters,
                Some(entry_function.origin().clone()),
            )
            .with_detail("entry"),
        ])
        .unwrap_or_else(|| NotLowered::single(Form::Parameters)));
    }
    let class =
        types::class_of(&entry_function.signature().result()).map_err(|kind| {
            NotLowered::type_of(kind, Some(entry_function.origin().clone()))
        })?;

    // The lambdas of the program follow its functions and module values in the
    // table, in the order a traversal meets them.
    let lambdas = lower::collect(program);
    let total = u32::try_from(lambdas.len())
        .ok()
        .and_then(|lambdas| total.checked_add(lambdas))
        .ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    let fns = encode::routines_for(total);
    // A call pushes a frame of the callee's size, which is known only once the
    // callee is lowered: the first pass finds every size.
    let count = total as usize;
    let mut first = lower::Lowering::new(&fns, program, &lambdas, vec![0; count]);
    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        sizes.push(first.function(index)?.slots);
    }
    let mut lowering = lower::Lowering::new(&fns, program, &lambdas, sizes.clone());
    let mut lowered = Vec::with_capacity(count);
    for index in 0..count {
        lowered.push(lowering.function(index)?.code);
    }
    let entry_slots = sizes
        .get(entry as usize)
        .copied()
        .ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    let include_new = lowering.built_object() || globals != 0;
    let parts = encode::Parts {
        functions: lowered,
        table: (0..total).collect(),
        entry: encode::Entry::Program {
            function: entry,
            slots: entry_slots,
            module_values: globals,
            class,
        },
        segments: lowering.into_segments(),
        include_new,
    };
    let bytes =
        encode::module(&parts).ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    Ok(EmittedModule {
        bytes,
        origins: OriginTable::default(),
    })
}
