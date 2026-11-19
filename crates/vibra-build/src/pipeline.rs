//! The build pipeline: a checked program in, an executable out.

use std::path::{Path, PathBuf};

use vibra_codegen::{Inputs, Target, emit};
use vibra_exec::{Program, verified};
use vibra_ir::{CheckedProgram, Type, TypeBody, vocabulary};

use crate::error::BuildError;
use crate::link::{LinkCommand, link_command, link_environment, run_link};
use crate::system;
use crate::toolchain::Toolchain;

/// The file name of the object inside the work directory.
pub const OBJECT_FILE: &str = "vibra-program.obj";

/// What a build needs besides the program. Every path is the caller's.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// The target. Only [`Target::WINDOWS_X64`] can be linked, and only on a
    /// Windows host.
    pub target: Target,
    /// The linker and libraries, from [`Toolchain::discover`].
    pub toolchain: Toolchain,
    /// The runtime archive (`vibra_nexus_bridge.lib`). How a compiler finds it
    /// is not this crate's concern.
    pub archive: PathBuf,
    /// A directory the build may create and write into: the object file and the
    /// linker's temporary files go here and nowhere else.
    pub work_dir: PathBuf,
    /// The executable to write. Its parent directory is created.
    pub output: PathBuf,
}

/// The result of a build.
#[derive(Clone, Debug)]
pub struct Built {
    /// The executable, which exists.
    pub executable: PathBuf,
    /// The object file the generator wrote, in the work directory.
    pub object: PathBuf,
    /// The link command that produced the executable.
    pub command: LinkCommand,
    /// The two numbers the generator was given.
    pub inputs: Inputs,
    /// What the linker printed; empty when it printed nothing.
    pub link_output: String,
}

/// The two numbers the code generator cannot take from the execution IR,
/// computed from the checked program.
///
/// - `registry_minor` is the lowered program's own
///   ([`Program::registry_minor`]).
/// - `entry_outcome` is `0` when the entry's result type is not an enum with a
///   variant named `err`, and `1 + index` of that variant otherwise, the same
///   rule by which the stepper and the interpreter classify an entry's value
///   as a program failure (`ObservedValue::is_err_variant`: the variant is
///   called `err`, whatever its content).
#[must_use]
pub fn inputs_for(checked: &CheckedProgram, lowered: &Program) -> Inputs {
    Inputs {
        entry_outcome: entry_outcome(checked),
        registry_minor: lowered.registry_minor(),
    }
}

/// The `entry_outcome` of the entry of `checked`: see [`inputs_for`].
#[must_use]
pub fn entry_outcome(checked: &CheckedProgram) -> u32 {
    let result = checked.entry().signature().result();
    let body = match &result {
        Type::Enum(variants) => Some(TypeBody::Enum(variants.clone())),
        Type::Declared(id) => checked
            .types()
            .iter()
            .find(|definition| definition.id() == id)
            .and_then(|definition| definition.instantiate(&[])),
        Type::Applied(id, arguments) => checked
            .types()
            .iter()
            .find(|definition| definition.id() == id)
            .and_then(|definition| definition.instantiate(arguments)),
        _ => None,
    };
    let Some(TypeBody::Enum(variants)) = body else {
        return 0;
    };
    variants
        .iter()
        .position(|(name, _)| name == vocabulary::ERR)
        .and_then(|index| u32::try_from(index + 1).ok())
        .unwrap_or(0)
}

/// Lowers, verifies, generates, and links `checked`.
///
/// # Errors
///
/// [`BuildError`]: the host or target is unsupported, the lowering, the
/// verifier, or the generator refused the program, a file could not be
/// written, or the linker failed. No executable remains at the output path
/// after an error.
pub fn build_executable(
    checked: &CheckedProgram,
    options: &BuildOptions,
) -> Result<Built, BuildError> {
    require_supported(options.target)?;
    let lowered = vibra_lower::lower(checked).map_err(BuildError::Lower)?;
    let inputs = inputs_for(checked, &lowered);
    build_program(&lowered, inputs, options)
}

/// As [`build_executable`], from lowered execution IR and the generator's
/// inputs. [`build_executable`] is the lowering and [`inputs_for`] before this.
///
/// # Errors
///
/// As [`build_executable`], except for the lowering.
pub fn build_program(
    program: &Program,
    inputs: Inputs,
    options: &BuildOptions,
) -> Result<Built, BuildError> {
    require_supported(options.target)?;
    // `link.exe` does not report a missing library given by a `\\?\` path: it
    // drops it, and the link fails later with unresolved symbols. The archive
    // is the driver's input, so it is checked here, where the cause is known.
    if !system::is_file(&options.archive) {
        return Err(BuildError::Io {
            action: "find the runtime archive",
            path: options.archive.clone(),
            message: "no such file".to_owned(),
        });
    }
    let proof = verified(program).map_err(BuildError::Verify)?;
    let object = emit(proof, &options.target, &inputs).map_err(BuildError::Emit)?;
    system::create_dir_all(&options.work_dir)?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        system::create_dir_all(parent)?;
    }
    let object_path = options.work_dir.join(OBJECT_FILE);
    system::write_file(&object_path, object.as_bytes())?;
    let command = link_command(
        &options.toolchain,
        &object_path,
        &options.archive,
        &options.output,
    );
    let link_output = link_object(&command, &options.work_dir, &options.output)?;
    Ok(Built {
        executable: options.output.clone(),
        object: object_path,
        command,
        inputs,
        link_output,
    })
}

/// Runs `command` in the linker environment of `work_dir`, after removing any
/// file at `output`, and removes `output` again if the link fails: a failed
/// link leaves no executable, and a stale one is never mistaken for a new one.
///
/// # Errors
///
/// [`BuildError::LinkFailed`], or [`BuildError::Io`] when `output` cannot be
/// removed.
pub fn link_object(
    command: &LinkCommand,
    work_dir: &Path,
    output: &Path,
) -> Result<String, BuildError> {
    system::remove_file(output)?;
    let result = run_link(command, &link_environment(work_dir));
    if result.is_err() {
        system::remove_file(output)?;
    }
    result
}

fn require_supported(target: Target) -> Result<(), BuildError> {
    if target == Target::WINDOWS_X64 && system::host_is_windows() {
        Ok(())
    } else {
        Err(BuildError::Unsupported {
            host: system::host_os(),
            target: target.triple(),
        })
    }
}
