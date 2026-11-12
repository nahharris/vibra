//! The build driver: toolchain discovery and the link driver (package X3-07 of
//! the Nexus plan).
//!
//! A checked program goes through [`vibra_lower::lower`],
//! [`vibra_exec::verified`], and [`vibra_codegen::emit`] to an object file, and
//! the MSVC `link.exe` joins that object with the runtime archive into an
//! executable. [`build_executable`] does all of it; the steps are public so a
//! test can run one.
//!
//! - [`Sources`] holds the ambient facts discovery starts from, and
//!   [`Sources::from_machine`] is the one function that reads them.
//!   [`Toolchain::discover`] is a function of a `Sources` and the filesystem.
//! - [`link_command`] builds the link command as a value, from a
//!   [`Toolchain`] and three paths; the linker runs without a shell, in the
//!   environment of [`link_environment`].
//! - Only `x86_64-pc-windows-msvc` on a Windows host is implemented. On any
//!   other host or target the driver answers [`BuildError::Unsupported`]. The
//!   crate compiles everywhere.
//! - The runtime archive is an input: [`BuildOptions::archive`].
//!
//! The crate reads the environment, starts the linker, and writes files, all in
//! the module `system`, which no other module of the crate may duplicate.
//!
//! The typed errors [`BuildError::MissingLinkTool`] and
//! [`BuildError::LinkFailed`] are the planned diagnostics `@tool.missing-link-tool`
//! and `@tool.link-failed` of the specification package X3-S1; the binding to
//! the diagnostics registry is owed after that package merges.

mod error;
mod link;
mod pipeline;
mod sources;
mod system;
mod toolchain;

pub use error::BuildError;
pub use link::{
    LinkCommand, SYSTEM_LIBRARIES, link_command, link_environment, run_link,
};
pub use pipeline::{
    BuildOptions, Built, OBJECT_FILE, build_executable, build_program, entry_outcome,
    inputs_for, link_object,
};
pub use sources::{Sources, parse_installation_path, parse_kits_root};
pub use toolchain::Toolchain;
pub use vibra_codegen::{Inputs, Target};
