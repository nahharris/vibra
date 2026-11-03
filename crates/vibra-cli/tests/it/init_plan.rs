//! Service-level conflict tests for project initialization plans.

use std::fs;
use std::io;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

use vibra_cli::{InitError, apply_init, plan_init};

use crate::support::TempDir;

fn link_directory(link: &Path, target: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let link = link.to_string_lossy().replace('/', "\\");
        let target = target.to_string_lossy().replace('/', "\\");
        let command = format!("mklink /J {link} {target}");
        let output = Command::new("cmd").arg("/C").arg(command).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory links are unavailable on this platform",
        ))
    }
}

#[test]
fn init_plan_rechecks_a_race_created_destination_before_installing() {
    let root = TempDir::new("init-plan");
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
    let root = TempDir::new("init-plan");
    let error = plan_init(root.path(), Some(Path::new("../outside")))
        .expect_err("parent traversal must be refused");
    assert!(matches!(error, InitError::InvalidInput(_)));
}

#[test]
fn init_rejects_a_parent_replaced_by_an_outside_directory_link() {
    let mut root = TempDir::new("init-plan");
    let parent = root.path().join("apps");
    fs::create_dir(&parent).expect("create destination parent");
    let outside = root.sibling_dir("outside");
    let moved_parent = outside.join("moved-apps");
    let sentinel = outside.join("sentinel");
    fs::write(&sentinel, b"keep").expect("write outside sentinel");
    let plan = plan_init(root.path(), Some(Path::new("apps/demo")))
        .expect("plan destination before parent replacement");

    fs::rename(&parent, &moved_parent)
        .expect("move original destination parent outside");
    link_directory(&parent, &outside)
        .expect("replace destination parent with directory link");

    let error =
        apply_init(&plan).expect_err("init must re-confine every parent component");

    assert!(matches!(error, InitError::InvalidInput(_)));
    assert_eq!(fs::read(sentinel).expect("read outside sentinel"), b"keep");
    assert!(!outside.join("demo/project.vibon").exists());
    assert!(!moved_parent.join("demo/project.vibon").exists());
}
