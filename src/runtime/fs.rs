//! Filesystem policy: explicit, domain-typed read/write capabilities.
//!
//! [`super::RunConfig::allow_read`](crate::runtime::RunConfig) and
//! [`super::RunConfig::allow_write`](crate::runtime::RunConfig) seed runtime-minted
//! filesystem capabilities passed by privileged stdlib helpers. The default is
//! **none** (stdout/stderr do not require filesystem capabilities).
//!
//! Runtime authorization canonicalizes the nearest existing path ancestor and
//! checks real path ancestry, so a scope for `root` does not authorize sibling
//! paths such as `root2`.
