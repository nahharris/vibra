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
//! scheme over a table of what each kind of pattern asks of a value. Steps 8 to
//! 10 lower collections, numbers and text, native bodies, interfaces, and the
//! natives. Step 11 lowers the test assertions and the test module
//! ([`emit_tests`]: `vibra_v1_test`, the failure exports, and the origin table of
//! [`OriginTable`]), and the last forms: a result converted through an array, a
//! dict, a record, or a union, and a `void` item of a builtin `option` iterator.
//! No form of a checked program is named by [`NotLowered`] since; it reports only
//! a program the checker cannot have produced, and never a module that omits part
//! of the program.
//!
//! # The module
//!
//! A module defines one 32-bit linear memory exported as `vibra_v1_memory`, one
//! function table holding its language functions, imports nothing (no program
//! lowered so far uses a native), exports the accessors of
//! `docs/spec/06-runtime.md`, "WebAssembly boundary", that do not depend on a
//! test, and has no custom section. Emission is deterministic: the bytes depend
//! only on the checked program.

mod canon;
mod canon_typed;
mod classify;
mod desc;
mod encode;
mod form;
mod key;
pub mod layout;
mod lower;
mod pattern;
mod primitive;
mod runtime;
pub mod support;
mod types;

use vibra_ir::{CheckedProgram, SourceOrigin, Type};

pub use desc::DescriptorTable;
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
    origins: Vec<(SourceOrigin, Option<Type>)>,
}

impl OriginTable {
    /// The number of origins the table maps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.origins.len()
    }

    /// Whether the table maps no origin: a module with no assertion call.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty()
    }

    /// The origin an ordinal names, or `None` for ordinal `0` and for an
    /// ordinal outside the table.
    #[must_use]
    pub fn origin(&self, ordinal: u32) -> Option<&SourceOrigin> {
        self.row(ordinal).map(|(origin, _)| origin)
    }

    /// The type by which the host reads the operands of a failed `assert.equal`
    /// at an ordinal: the static type of the operands, or `any` when the type
    /// names a generic parameter. `None` for every other origin.
    #[must_use]
    pub fn operand_type(&self, ordinal: u32) -> Option<&Type> {
        self.row(ordinal).and_then(|(_, ty)| ty.as_ref())
    }

    fn row(&self, ordinal: u32) -> Option<&(SourceOrigin, Option<Type>)> {
        let index = usize::try_from(ordinal).ok()?.checked_sub(1)?;
        self.origins.get(index)
    }
}

/// A module and the table that maps its origin ordinals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedModule {
    bytes: Vec<u8>,
    origins: OriginTable,
    descriptors: DescriptorTable,
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

    /// The table that names the types of the module's descriptors, produced
    /// beside it for a host that reads an interface value.
    #[must_use]
    pub const fn descriptors(&self) -> &DescriptorTable {
        &self.descriptors
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
    build(program, None)
}

/// Lowers the tests of a checked module set to a v1 test module.
///
/// `program` is any program of the set, which every test of a workspace shares,
/// and `entries` are the function indices of the tests, in canonical discovery
/// order: the module's `vibra_v1_test` runs the test at a position of this
/// list, each in a fresh instance. The same input always yields the same bytes.
///
/// # Errors
///
/// [`NotLowered`] as for [`emit`].
pub fn emit_tests(
    program: &CheckedProgram,
    entries: &[usize],
) -> Result<EmittedModule, NotLowered> {
    build(program, Some(entries))
}

#[allow(clippy::too_many_lines)]
fn build(
    program: &CheckedProgram,
    tests: Option<&[usize]>,
) -> Result<EmittedModule, NotLowered> {
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
    let mut class = runtime::ValueClass::Void;
    if tests.is_none() {
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
        class =
            types::class_of(&entry_function.signature().result()).map_err(|kind| {
                NotLowered::type_of(kind, Some(entry_function.origin().clone()))
            })?;
    }

    // The lambdas of the program follow its functions and module values in the
    // table, in the order a traversal meets them.
    let lambdas = lower::collect(program);
    let total = u32::try_from(lambdas.len())
        .ok()
        .and_then(|lambdas| total.checked_add(lambdas))
        .ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    // The key unit, the language function that orders keys by the descriptor of
    // their type, follows the lambdas in a module that calls it.
    let total = if lower::needs_key_unit(program) {
        total
            .checked_add(1)
            .ok_or_else(|| NotLowered::single(Form::ModuleSize))?
    } else {
        total
    };
    // A call pushes a frame of the callee's size, which is known only once the
    // callee is lowered: the first pass finds every size, and which natives the
    // program calls. The natives are imported, and an import takes a function
    // index before every defined function, so the second pass numbers the
    // routines after them.
    let count = total as usize;
    let run_tests = tests.is_some();
    let first_fns = encode::routines_for(total);
    let mut first = lower::Lowering::new(
        &first_fns,
        program,
        &lambdas,
        vec![0; count],
        Vec::new(),
        lower::Extras::default(),
        run_tests,
    );
    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        sizes.push(first.function(index)?.slots);
    }
    let natives = first.used_natives();
    let extras = first.needed();
    let imports = natives
        .iter()
        .filter_map(|intrinsic| intrinsic.native_import())
        .collect::<Vec<_>>();
    let import_count = u32::try_from(imports.len())
        .map_err(|_| NotLowered::single(Form::ModuleSize))?;
    let fns = encode::routines_for(
        total
            .checked_add(import_count)
            .ok_or_else(|| NotLowered::single(Form::ModuleSize))?,
    );
    let mut lowering = lower::Lowering::new(
        &fns,
        program,
        &lambdas,
        sizes.clone(),
        natives,
        extras,
        run_tests,
    );
    let mut lowered = Vec::with_capacity(count);
    for index in 0..count {
        lowered.push(lowering.function(index)?.code);
    }
    let entry_slots = sizes
        .get(entry as usize)
        .copied()
        .ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    let include_new = lowering.built_object() || globals != 0;
    let extra = lowering.comparer_routines();
    let descriptors = lowering.descriptors();
    let origins = OriginTable {
        origins: lowering.take_origins(),
    };
    let entry = match tests {
        None => encode::Entry::Program {
            function: entry,
            slots: entry_slots,
            module_values: globals,
            class,
        },
        Some(entries) => encode::Entry::Tests {
            entries: entries
                .iter()
                .map(|index| {
                    let function = program.functions().get(*index)?;
                    let signature = function.signature();
                    if signature.slot_types().is_empty()
                        && signature.result() == Type::Void
                    {
                        Some((u32::try_from(*index).ok()?, sizes.get(*index).copied()?))
                    } else {
                        None
                    }
                })
                .collect(),
            module_values: globals,
        },
    };
    let parts = encode::Parts {
        functions: lowered,
        // The table holds function indices, which the imports come before.
        table: (import_count..import_count + total).collect(),
        entry,
        segments: lowering.into_segments(),
        include_new,
        imports,
        extra,
    };
    let bytes =
        encode::module(&parts).ok_or_else(|| NotLowered::single(Form::ModuleSize))?;
    Ok(EmittedModule {
        bytes,
        origins,
        descriptors,
    })
}

/// Whether the emitter lowers the registry row `intrinsic` inline: a primitive
/// row that does not loop, which is the integer rows but `to-str` and `parse`,
/// the integer conversions, the `char` rows, `array.length`, and the float
/// arithmetic, equality, and order rows. The rows that loop are native imports
/// (`CompilerIntrinsic::native_import`), and `vibra_ir::external::CompilerIntrinsic::vectors`
/// holds both kinds to samples. A row it lowers neither way is reported by
/// [`emit`] as [`Form::Malformed`].
#[must_use]
pub fn lowers_primitive(intrinsic: vibra_ir::external::CompilerIntrinsic) -> bool {
    primitive::plan(intrinsic).is_some()
}
