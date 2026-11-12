//! The launcher's `main` (decision N13): the process entry point that lives in
//! the static archive. It is the only place that reads the process.
//!
//! Kept to one function so that it sits alone in its own code-generation unit:
//! a test binary that links this crate as a library then never pulls it in,
//! and so never sees a second `main` or the undefined `vibra_program`.

use std::ffi::{c_char, c_int};

use crate::abi::VibraProgram;

unsafe extern "C" {
    /// Defined by the generated object.
    static vibra_program: VibraProgram;
}

/// Which launcher profile this build contains.
pub const PROFILE: &str = if cfg!(feature = "harness") {
    "harness"
} else {
    "system"
};

/// The process entry point. `system`: runs the program on the system provider
/// over the process arguments and exits with the code of the tooling chapter.
/// With the feature `harness`: the first argument is an optional quota, the
/// host world is read from standard input, and one observation document is
/// written to standard output.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    // SAFETY: the generated object defines `vibra_program` as a read-only
    // `VibraProgram` that lives as long as the process; the link fails if it
    // does not.
    let program = unsafe { &vibra_program };
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    #[cfg(not(feature = "harness"))]
    {
        // SAFETY: the code generator emitted the descriptor and its tables as
        // read-only data, which is the contract of `launch_system`.
        unsafe {
            crate::launcher::launch_system(program, arguments, &mut std::io::stderr())
        }
    }
    #[cfg(feature = "harness")]
    {
        use std::io::Read;
        let quota = arguments
            .first()
            .and_then(|argument| argument.to_str())
            .and_then(|argument| argument.parse::<u64>().ok())
            .unwrap_or(vibra_nexus::UNBOUNDED);
        let mut world = String::new();
        let _ = std::io::stdin().read_to_string(&mut world);
        // SAFETY: as above.
        unsafe {
            crate::harness::launch_harness(
                program,
                &world,
                quota,
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            )
        }
    }
}
