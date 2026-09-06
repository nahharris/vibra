//! Workspace-owned project data and later project orchestration.
//!
//! The first workspace slice is intentionally small: [`project`] decodes a
//! parsed VIBON [`vibra_syntax::DataNode`] into the closed `@project.v1`
//! schema. It does not inspect the filesystem, resolve references, contact a
//! network, or generate a lock file.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod project;
