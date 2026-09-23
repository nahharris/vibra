//! Service-level conflict tests for project initialization plans.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_cli::{InitError, apply_init, plan_init};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "vibra-init-plan-step11-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn init_plan_rechecks_a_race_created_destination_before_installing() {
    let root = TempDir::new();
    let plan = plan_init(root.path(), Some(Path::new("created-later")))
        .expect("absent destination can be planned");
    let destination = root.path().join("created-later");
    fs::create_dir(&destination).expect("simulate destination race");
    fs::write(destination.join("sentinel"), b"keep").expect("write race sentinel");

    let error = apply_init(&plan).expect_err("stale destination must be refused");
    assert!(matches!(error, InitError::InvalidInput(_)));
    assert_eq!(
        fs::read(destination.join("sentinel")).expect("read race sentinel"),
        b"keep"
    );
    assert!(!destination.join("project.vibon").exists());
}

#[test]
fn init_plan_rejects_destinations_that_escape_the_workspace() {
    let root = TempDir::new();
    let error = plan_init(root.path(), Some(Path::new("../outside")))
        .expect_err("parent traversal must be refused");
    assert!(matches!(error, InitError::InvalidInput(_)));
}
