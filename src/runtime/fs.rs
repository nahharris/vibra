//! Filesystem policy: explicit read/write grants and legacy preopen compatibility.
//!
//! [`super::RunConfig::allow_read`](crate::runtime::RunConfig) and
//! [`super::RunConfig::allow_write`](crate::runtime::RunConfig) seed runtime-minted
//! filesystem grants exposed through `stdlib/security.vibra`. The default is
//! **none** (stdout/stderr do not require grants). Legacy
//! `preopen_host_dirs` entries seed both read and write grants for compatibility.
//!
//! Runtime authorization canonicalizes the nearest existing path ancestor and
//! checks real path ancestry, so a grant for `root` does not authorize sibling
//! paths such as `root2`.
