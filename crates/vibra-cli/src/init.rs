//! Typed planning and application for `vibra project init`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_fmt::format_source;
use vibra_syntax::{parse_data, parse_source};
use vibra_types::check_source;
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::confined_fs::ConfinedDir;

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// A failure while planning or applying project initialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitError {
    /// The destination or project name violates the initialization contract.
    InvalidInput(String),
    /// A filesystem operation failed.
    OperationalFailure(String),
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) | Self::OperationalFailure(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for InitError {}

/// A previewable project initialization layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitPlan {
    workspace_root: PathBuf,
    destination: PathBuf,
    relative_destination: PathBuf,
    expected_state: DestinationState,
    package_name: String,
    source_path: String,
    project_text: String,
    source_text: String,
    created_paths: Vec<String>,
}

impl InitPlan {
    /// Canonical path that will contain the initialized project.
    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.destination
    }

    /// Workspace-relative destination used in this plan.
    #[must_use]
    pub fn relative_destination(&self) -> &Path {
        &self.relative_destination
    }

    /// Canonical package and target name derived from the destination.
    #[must_use]
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// Every directory and file created, relative to the initialized root.
    #[must_use]
    pub fn created_paths(&self) -> &[String] {
        &self.created_paths
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestinationState {
    Absent,
    Empty,
}

#[derive(Debug)]
struct InitStage {
    parent: ConfinedDir,
    name: OsString,
    directory: ConfinedDir,
}

/// Plans the canonical minimum project layout without changing the filesystem.
pub fn plan_init(
    workspace_root: impl AsRef<Path>,
    destination: Option<&Path>,
) -> Result<InitPlan, InitError> {
    let workspace_root =
        fs::canonicalize(workspace_root.as_ref()).map_err(|error| {
            InitError::InvalidInput(format!(
                "cannot resolve workspace directory {}: {error}",
                workspace_root.as_ref().display()
            ))
        })?;
    if !workspace_root.is_dir() {
        return Err(InitError::InvalidInput(format!(
            "workspace is not a directory: {}",
            workspace_root.display()
        )));
    }
    ConfinedDir::open(&workspace_root).map_err(|error| {
        InitError::InvalidInput(format!(
            "workspace path contains a symbolic link or reparse point: {error}"
        ))
    })?;
    let relative_destination = normalize_destination(destination)?;
    let (path, expected_state) =
        inspect_destination(&workspace_root, &relative_destination)?;
    let package_name = package_name(&workspace_root, &relative_destination)?;
    let source_path = format!("src/{package_name}/main.vib");
    let project_text = format!(
        "(record format: @project.v1 package: (record name: \"{package_name}\" version: \"0.1.0\") targets: (array (record name: @{package_name} kind: @bin root: \"src/{package_name}\" entry: @{package_name}.main.main effects: (array))) dependencies: (map))"
    );
    let project_text = format_source("project.vibon", &project_text)
        .map_err(|error| InitError::OperationalFailure(error.to_string()))?;
    let source_text = format_source(&source_path, "(defn main () void (do))")
        .map_err(|error| InitError::OperationalFailure(error.to_string()))?;
    validate_generated_documents(&project_text, &source_path, &source_text)?;

    let created_paths = vec![
        "project.vibon".to_owned(),
        "src".to_owned(),
        format!("src/{package_name}"),
        source_path.clone(),
        "tests".to_owned(),
    ];
    Ok(InitPlan {
        workspace_root,
        destination: path,
        relative_destination,
        expected_state,
        package_name,
        source_path,
        project_text,
        source_text,
        created_paths,
    })
}

/// Applies the prepared initialization after checking destination conflicts again.
pub fn apply_init(plan: &InitPlan) -> Result<(), InitError> {
    let stage = create_stage(plan)?;
    let result = install_stage(plan, &stage);
    if result.is_err() {
        cleanup_stage(&stage, plan);
    }
    result
}

fn normalize_destination(destination: Option<&Path>) -> Result<PathBuf, InitError> {
    let Some(destination) = destination else {
        return Ok(PathBuf::new());
    };
    if destination.is_absolute() {
        return Err(InitError::InvalidInput(
            "project destination must be workspace-relative".to_owned(),
        ));
    }
    let mut normalized = PathBuf::new();
    for component in destination.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(InitError::InvalidInput(
                    "project destination may not escape the workspace".to_owned(),
                ));
            }
        }
    }
    Ok(normalized)
}

fn inspect_destination(
    workspace_root: &Path,
    relative_destination: &Path,
) -> Result<(PathBuf, DestinationState), InitError> {
    let root = ConfinedDir::open(workspace_root).map_err(|error| {
        InitError::InvalidInput(format!("cannot safely open workspace: {error}"))
    })?;
    if relative_destination.as_os_str().is_empty() {
        if root.is_empty().map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot inspect project destination {}: {error}",
                workspace_root.display()
            ))
        })? {
            return Ok((workspace_root.to_path_buf(), DestinationState::Empty));
        }
        return Err(InitError::InvalidInput(format!(
            "project destination is not empty: {}",
            workspace_root.display()
        )));
    }
    let parent_relative = relative_destination
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let parent = root.open_dir(parent_relative).map_err(|error| {
        InitError::InvalidInput(format!(
            "project destination parent is unavailable or contains a link: {error}"
        ))
    })?;
    let name = relative_destination.file_name().ok_or_else(|| {
        InitError::InvalidInput("project destination must name a directory".to_owned())
    })?;
    let destination = workspace_root.join(relative_destination);
    match parent.open_dir(name) {
        Ok(existing)
            if existing.is_empty().map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot inspect project destination {}: {error}",
                    destination.display()
                ))
            })? =>
        {
            Ok((destination, DestinationState::Empty))
        }
        Ok(_) => Err(InitError::InvalidInput(format!(
            "project destination is not empty: {}",
            destination.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((destination, DestinationState::Absent))
        }
        Err(error) => Err(InitError::InvalidInput(format!(
            "project destination is not a safe directory: {error}"
        ))),
    }
}

fn package_name(
    workspace_root: &Path,
    relative_destination: &Path,
) -> Result<String, InitError> {
    let original = relative_destination
        .file_name()
        .or_else(|| workspace_root.file_name())
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| {
            InitError::InvalidInput(
                "project destination must have a Unicode directory name".to_owned(),
            )
        })?;
    let mut result = String::new();
    let mut separator = false;
    for character in original.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    if result.is_empty() {
        result.push_str("hello");
    }
    if result
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        result.insert_str(0, "project-");
    }
    Ok(result)
}

fn validate_generated_documents(
    project_text: &str,
    source_path: &str,
    source_text: &str,
) -> Result<(), InitError> {
    let project = parse_data(Path::new("project.vibon"), project_text)
        .map_err(|error| InitError::OperationalFailure(error.to_string()))?;
    if !project.accepted() || project.data().is_none() {
        return Err(InitError::OperationalFailure(
            "generated project manifest did not pass the VIBON reader".to_owned(),
        ));
    }
    let source = parse_source(Path::new(source_path), source_text)
        .map_err(|error| InitError::OperationalFailure(error.to_string()))?;
    if !source.accepted() || source.recovered() {
        return Err(InitError::OperationalFailure(
            "generated entry did not pass the source reader".to_owned(),
        ));
    }
    let checked = check_source(source_path, source_text);
    if !checked.accepted() || source_text.contains("@std.") {
        return Err(InitError::OperationalFailure(
            "generated entry is not an admitted pure M2 program".to_owned(),
        ));
    }
    Ok(())
}

fn destination_parent(plan: &InitPlan) -> Result<(ConfinedDir, OsString), InitError> {
    if plan.relative_destination.as_os_str().is_empty() {
        let parent_path = plan.workspace_root.parent().ok_or_else(|| {
            InitError::InvalidInput("workspace root has no parent directory".to_owned())
        })?;
        let name = plan.workspace_root.file_name().ok_or_else(|| {
            InitError::InvalidInput("workspace root has no directory name".to_owned())
        })?;
        return ConfinedDir::open(parent_path)
            .map(|parent| (parent, OsString::from(name)))
            .map_err(|error| {
                InitError::InvalidInput(format!(
                    "cannot safely open workspace parent: {error}"
                ))
            });
    }
    let root = ConfinedDir::open(&plan.workspace_root).map_err(|error| {
        InitError::InvalidInput(format!("cannot safely reopen workspace: {error}"))
    })?;
    let parent_relative = plan
        .relative_destination
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let parent = root.open_dir(parent_relative).map_err(|error| {
        InitError::InvalidInput(format!("project destination parent changed: {error}"))
    })?;
    let name = plan.relative_destination.file_name().ok_or_else(|| {
        InitError::InvalidInput("project destination has no name".to_owned())
    })?;
    Ok((parent, OsString::from(name)))
}

fn create_stage(plan: &InitPlan) -> Result<InitStage, InitError> {
    let (parent, _) = destination_parent(plan)?;
    for _ in 0..32 {
        let nonce = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let name =
            OsString::from(format!(".vibra-init-{}-{nonce}.tmp", std::process::id()));
        match parent.create_dir(&name) {
            Ok(()) => {
                let directory = match parent.open_dir(&name) {
                    Ok(directory) => directory,
                    Err(error) => {
                        let _ = parent.remove_dir(&name);
                        return Err(InitError::OperationalFailure(format!(
                            "cannot safely open initialization stage: {error}"
                        )));
                    }
                };
                let stage = InitStage {
                    parent,
                    name,
                    directory,
                };
                if let Err(error) = populate_stage(plan, &stage.directory) {
                    cleanup_stage(&stage, plan);
                    return Err(error);
                }
                return Ok(stage);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(InitError::OperationalFailure(format!(
                    "cannot create initialization staging directory {}: {error}",
                    parent.path().join(&name).display()
                )));
            }
        }
    }
    Err(InitError::OperationalFailure(
        "cannot allocate a unique initialization staging directory".to_owned(),
    ))
}

fn populate_stage(plan: &InitPlan, stage: &ConfinedDir) -> Result<(), InitError> {
    let src_name = OsStr::new("src");
    let package_name = OsStr::new(&plan.package_name);
    let tests_name = OsStr::new("tests");
    stage.create_dir(src_name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot prepare staged source root: {error}"
        ))
    })?;
    stage.create_dir(tests_name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot prepare staged tests directory: {error}"
        ))
    })?;
    let src = stage.open_dir("src").map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot open staged source root: {error}"
        ))
    })?;
    src.create_dir(package_name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot prepare staged package directory: {error}"
        ))
    })?;
    let package = src.open_dir(package_name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot open staged package directory: {error}"
        ))
    })?;
    stage
        .write_new_file(
            OsStr::new("project.vibon"),
            plan.project_text.as_bytes(),
            None,
        )
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot write staged project manifest: {error}"
            ))
        })?;
    package
        .write_new_file(OsStr::new("main.vib"), plan.source_text.as_bytes(), None)
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot write staged source entry: {error}"
            ))
        })?;

    let snapshot = WorkspaceSnapshot::load_confined(stage.path()).map_err(|error| {
        InitError::OperationalFailure(format!(
            "generated project failed workspace validation: {error}"
        ))
    })?;
    let resolved = snapshot.resolve().map_err(|error| {
        InitError::OperationalFailure(format!(
            "generated project failed entry validation: {error}"
        ))
    })?;
    if !resolved.accepted() {
        return Err(InitError::OperationalFailure(
            "generated project entry did not resolve".to_owned(),
        ));
    }
    let Some(document) = snapshot
        .source()
        .documents()
        .find(|document| document.source_id() == plan.source_path)
    else {
        return Err(InitError::OperationalFailure(
            "generated source is absent from the workspace snapshot".to_owned(),
        ));
    };
    let text = std::str::from_utf8(document.bytes()).map_err(|error| {
        InitError::OperationalFailure(format!("generated source is not UTF-8: {error}"))
    })?;
    if !check_source(plan.source_path.as_str(), text).accepted() {
        return Err(InitError::OperationalFailure(
            "generated entry failed semantic checking".to_owned(),
        ));
    }
    if !snapshot.project().project().dependencies().is_empty() {
        return Err(InitError::OperationalFailure(
            "generated project unexpectedly declares dependencies".to_owned(),
        ));
    }
    Ok(())
}

fn install_stage(plan: &InitPlan, stage: &InitStage) -> Result<(), InitError> {
    install_stage_with_hook(plan, stage, || Ok(()))
}

fn install_stage_with_hook(
    plan: &InitPlan,
    stage: &InitStage,
    before_publish: impl FnOnce() -> Result<(), InitError>,
) -> Result<(), InitError> {
    if !stage
        .parent
        .is_same_directory(&stage.name, &stage.directory)
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot verify staging directory: {error}"
            ))
        })?
    {
        return Err(InitError::InvalidInput(
            "initialization staging path changed before installation".to_owned(),
        ));
    }
    let (destination_parent, destination_name) = destination_parent(plan)?;
    if !stage.parent.same_as(&destination_parent).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot verify project destination parent: {error}"
        ))
    })? {
        return Err(InitError::InvalidInput(
            "project destination parent changed after planning".to_owned(),
        ));
    }
    recheck_destination(plan, &destination_parent, &destination_name)?;
    if plan.expected_state == DestinationState::Absent {
        stage
            .parent
            .rename_noreplace_to(&stage.name, &destination_parent, &destination_name)
            .map_err(|error| {
                InitError::InvalidInput(format!(
                    "project destination became unavailable or conflicting: {error}"
                ))
            })?;
        return Ok(());
    }

    let backup_name = OsString::from(format!(
        ".vibra-init-backup-{}-{}.tmp",
        std::process::id(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    ));
    destination_parent
        .rename_noreplace_to(&destination_name, &destination_parent, &backup_name)
        .map_err(|error| {
            InitError::InvalidInput(format!(
                "project destination changed before atomic install: {error}"
            ))
        })?;

    let backup_is_empty = destination_parent
        .open_dir(&backup_name)
        .and_then(|backup| backup.is_empty())
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot validate moved project destination: {error}"
            ))
        })?;
    if !backup_is_empty {
        restore_backup(&destination_parent, &backup_name, &destination_name)?;
        return Err(InitError::InvalidInput(
            "project destination became nonempty after planning".to_owned(),
        ));
    }
    if let Err(error) = before_publish() {
        restore_backup(&destination_parent, &backup_name, &destination_name)?;
        return Err(error);
    }
    if let Err(error) = stage.parent.rename_noreplace_to(
        &stage.name,
        &destination_parent,
        &destination_name,
    ) {
        restore_backup(&destination_parent, &backup_name, &destination_name)?;
        return Err(InitError::OperationalFailure(format!(
            "cannot publish staged project tree: {error}"
        )));
    }
    destination_parent
        .remove_dir(&backup_name)
        .map_err(|error| InitError::OperationalFailure(format!(
            "project initialized, but the empty destination backup could not be removed: {error}"
        )))?;
    Ok(())
}

fn restore_backup(
    parent: &ConfinedDir,
    backup_name: &OsStr,
    destination_name: &OsStr,
) -> Result<(), InitError> {
    parent
        .rename_noreplace_to(backup_name, parent, destination_name)
        .map_err(|error| InitError::OperationalFailure(format!(
            "cannot restore the original empty destination after failed project installation: {error}"
        )))
}

fn recheck_destination(
    plan: &InitPlan,
    parent: &ConfinedDir,
    name: &OsStr,
) -> Result<(), InitError> {
    match (plan.expected_state, parent.open_dir(name)) {
        (DestinationState::Absent, Err(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(())
        }
        (DestinationState::Absent, Ok(_)) => Err(InitError::InvalidInput(format!(
            "project destination was created after planning: {}",
            plan.destination.display()
        ))),
        (DestinationState::Empty, Ok(directory))
            if directory.is_empty().map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot recheck project destination: {error}"
                ))
            })? =>
        {
            Ok(())
        }
        (DestinationState::Empty, Ok(_)) => Err(InitError::InvalidInput(format!(
            "project destination became nonempty after planning: {}",
            plan.destination.display()
        ))),
        (_, Err(error)) => Err(InitError::InvalidInput(format!(
            "project destination changed after planning: {error}"
        ))),
    }
}

fn cleanup_stage(stage: &InitStage, plan: &InitPlan) {
    if let Ok(src) = stage.directory.open_dir("src") {
        if let Ok(package) = src.open_dir(&plan.package_name) {
            let _ = package.remove_file(OsStr::new("main.vib"));
        }
        let _ = src.remove_dir(OsStr::new(&plan.package_name));
    }
    let _ = stage.directory.remove_dir(OsStr::new("src"));
    let _ = stage.directory.remove_dir(OsStr::new("tests"));
    let _ = stage.directory.remove_file(OsStr::new("project.vibon"));
    if stage
        .parent
        .is_same_directory(&stage.name, &stage.directory)
        .unwrap_or(false)
    {
        let _ = stage.parent.remove_dir(&stage.name);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use super::{
        InitError, apply_init, cleanup_stage, create_stage, install_stage_with_hook,
        plan_init,
    };
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempWorkspace(std::path::PathBuf);

    impl TempWorkspace {
        fn new() -> Self {
            let nonce = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vibra-init-unit-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("create workspace");
            Self(path)
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn empty_destination_is_restored_when_installation_fails_before_publish() {
        let root = TempWorkspace::new();
        let destination = root.0.join("demo");
        fs::create_dir(&destination).expect("create empty destination");
        let plan = plan_init(&root.0, Some(Path::new("demo")))
            .expect("plan initialization into the empty destination");

        let stage = create_stage(&plan).expect("prepare initialization staging tree");
        let result = install_stage_with_hook(&plan, &stage, || {
            Err(InitError::OperationalFailure(
                "injected pre-publish failure".to_owned(),
            ))
        });
        if result.is_err() {
            cleanup_stage(&stage, &plan);
        }
        let error = result.expect_err("injected install failure is reported");

        assert!(matches!(error, InitError::OperationalFailure(_)));
        assert_eq!(
            fs::read_dir(&destination)
                .expect("read restored destination")
                .count(),
            0,
            "the original empty destination remains empty"
        );
        let entries = fs::read_dir(&root.0)
            .expect("read workspace after rollback")
            .map(|entry| entry.expect("read workspace entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("demo")]);
    }

    #[test]
    fn apply_init_wrapper_still_installs_a_prepared_project() {
        let root = TempWorkspace::new();
        let plan = plan_init(&root.0, Some(Path::new("demo")))
            .expect("plan initialization into an absent destination");

        apply_init(&plan).expect("install staged project atomically");

        assert!(root.0.join("demo/project.vibon").is_file());
        assert!(root.0.join("demo/src/demo/main.vib").is_file());
        assert!(root.0.join("demo/tests").is_dir());
    }
}
