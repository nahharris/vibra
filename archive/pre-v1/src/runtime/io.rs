//! Stdio policy for the WASI runner (fd layout matches WASI preview1 / POSIX).
//!
//! Host code should treat **0 / 1 / 2** as stdin / stdout / stderr. The guest sees the same fds
//! when using [`wasi_snapshot_preview1`](https://github.com/WebAssembly/WASI).

/// Standard input fd.
pub const STDIN_FD: i32 = 0;
/// Standard output fd.
pub const STDOUT_FD: i32 = 1;
/// Standard error fd.
pub const STDERR_FD: i32 = 2;
