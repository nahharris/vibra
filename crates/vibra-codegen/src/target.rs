//! The targets the generator emits for, and the one place that fixes the
//! Cranelift settings.

use cranelift_codegen::isa::{self, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable, Flags};

use crate::error::EmitError;

/// The triples [`Target::new`] accepts.
pub const SUPPORTED_TRIPLES: &[&str] =
    &["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"];

/// The Cranelift settings of every target, stated once. Output depends on the
/// triple and on these values only: the ISA is found by name and no host CPU
/// is detected, so the architecture baseline is used and the object is the
/// same on every build machine.
///
/// - `opt_level = none`: nothing is optimized before parity with the stepper
///   is established (the plan forbids it).
/// - `is_pic = true`: the object links into a position-independent
///   executable on every target.
/// - `enable_verifier = true`: Cranelift checks every function it is given.
/// - `unwind_info = true`: the Windows x64 object needs unwind data for the
///   frames of the step functions.
/// - Every other value restates the default of Cranelift 0.136.2, so a change
///   of Cranelift's defaults cannot change the object without a change here.
const FLAGS: &[(&str, &str)] = &[
    ("opt_level", "none"),
    ("is_pic", "true"),
    ("enable_verifier", "true"),
    ("unwind_info", "true"),
    ("regalloc_algorithm", "backtracking"),
    ("regalloc_checker", "false"),
    ("regalloc_verbose_logs", "false"),
    ("enable_alias_analysis", "true"),
    ("enable_nan_canonicalization", "false"),
    ("enable_pinned_reg", "false"),
    ("enable_llvm_abi_extensions", "false"),
    ("enable_multi_ret_implicit_sret", "false"),
    ("use_colocated_libcalls", "false"),
    ("libcall_call_conv", "isa_default"),
    ("tls_model", "none"),
    ("stack_switch_model", "none"),
    ("preserve_frame_pointers", "false"),
    ("machine_code_cfg_info", "false"),
    ("enable_probestack", "false"),
    ("probestack_strategy", "outline"),
    ("probestack_size_log2", "12"),
    ("enable_heap_access_spectre_mitigation", "true"),
    ("enable_table_access_spectre_mitigation", "true"),
    ("enable_incremental_compilation_cache_checks", "false"),
    ("enable_compact_unwind_abi", "false"),
    ("bb_padding_log2_minus_one", "0"),
    ("log2_min_function_alignment", "0"),
];

/// A target triple the generator emits for, with the fixed flag set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    triple: &'static str,
}

impl Target {
    /// `x86_64-pc-windows-msvc`: a PE/COFF object.
    pub const WINDOWS_X64: Self = Self {
        triple: "x86_64-pc-windows-msvc",
    };
    /// `x86_64-unknown-linux-gnu`: an ELF object. Emitted by the same code path;
    /// nothing in this crate links or runs it.
    pub const LINUX_X64: Self = Self {
        triple: "x86_64-unknown-linux-gnu",
    };

    /// The target named by `triple`.
    ///
    /// # Errors
    ///
    /// [`EmitError::UnsupportedTarget`] for any triple outside
    /// [`SUPPORTED_TRIPLES`].
    pub fn new(triple: &str) -> Result<Self, EmitError> {
        [Self::WINDOWS_X64, Self::LINUX_X64]
            .into_iter()
            .find(|target| target.triple == triple)
            .ok_or_else(|| EmitError::UnsupportedTarget(triple.to_owned()))
    }

    /// The triple.
    #[must_use]
    pub const fn triple(&self) -> &'static str {
        self.triple
    }

    /// The Cranelift flags, as `(name, value)` pairs in the order they are set.
    #[must_use]
    pub const fn flags(&self) -> &'static [(&'static str, &'static str)] {
        FLAGS
    }

    pub(crate) fn isa(&self) -> Result<OwnedTargetIsa, EmitError> {
        let backend = |message: String| EmitError::Backend(message);
        let mut builder = settings::builder();
        for (name, value) in FLAGS {
            builder
                .set(name, value)
                .map_err(|error| backend(format!("flag `{name}`: {error}")))?;
        }
        let flags = Flags::new(builder);
        isa::lookup_by_name(self.triple)
            .map_err(|error| backend(format!("{}: {error}", self.triple)))?
            .finish(flags)
            .map_err(|error| backend(format!("{}: {error}", self.triple)))
    }
}
