//! Filesystem policy: preopened host directories and future sandbox knobs.
//!
//! [`super::RunConfig::preopen_host_dirs`](crate::runtime::RunConfig) controls which host paths are
//! visible under the WASI virtual root. The default is **none** (MVP `println` only needs stdio);
//! add entries when running programs that use `stdlib/fs` or other preopened paths.
//!
//! **MVP:** no extra checks beyond wasmer-wasix; absolute paths and `..` follow WASI host behavior.
