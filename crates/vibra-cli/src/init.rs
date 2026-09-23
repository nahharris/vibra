//! Typed planning and application for `vibra project init`.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_fmt::format_source;
use vibra_syntax::{parse_data, parse_source};
use vibra_types::check_source;
use vibra_workspace::WorkspaceSnapshot;

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
        let _ = fs::remove_dir_all(&stage);
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
    if relative_destination.as_os_str().is_empty() {
        return inspect_existing_destination(workspace_root, workspace_root);
    }
    let components = relative_destination.components().collect::<Vec<_>>();
    let mut current = workspace_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(InitError::InvalidInput(
                "project destination is not a normalized relative path".to_owned(),
            ));
        };
        current.push(name);
        let final_component = index == components.len().saturating_sub(1);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(InitError::InvalidInput(format!(
                    "project destination contains a symbolic link: {}",
                    current.display()
                )));
            }
            Ok(metadata) if final_component => {
                if !metadata.is_dir() {
                    return Err(InitError::InvalidInput(format!(
                        "project destination is not a directory: {}",
                        current.display()
                    )));
                }
                let canonical = fs::canonicalize(&current).map_err(|error| {
                    InitError::OperationalFailure(format!(
                        "cannot resolve project destination {}: {error}",
                        current.display()
                    ))
                })?;
                if !canonical.starts_with(workspace_root) {
                    return Err(InitError::InvalidInput(
                        "project destination resolves outside the workspace".to_owned(),
                    ));
                }
                return inspect_existing_destination(workspace_root, &canonical);
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(InitError::InvalidInput(format!(
                    "project destination parent is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !final_component {
                    return Err(InitError::InvalidInput(format!(
                        "project destination parent does not exist: {}",
                        current.display()
                    )));
                }
                let parent = current.parent().ok_or_else(|| {
                    InitError::InvalidInput(
                        "project destination has no parent directory".to_owned(),
                    )
                })?;
                let canonical_parent = fs::canonicalize(parent).map_err(|error| {
                    InitError::OperationalFailure(format!(
                        "cannot resolve project destination parent {}: {error}",
                        parent.display()
                    ))
                })?;
                if !canonical_parent.starts_with(workspace_root) {
                    return Err(InitError::InvalidInput(
                        "project destination resolves outside the workspace".to_owned(),
                    ));
                }
                return Ok((current, DestinationState::Absent));
            }
            Err(error) => {
                return Err(InitError::OperationalFailure(format!(
                    "cannot inspect project destination {}: {error}",
                    current.display()
                )));
            }
        }
    }
    Err(InitError::InvalidInput(
        "project destination must name a directory".to_owned(),
    ))
}

fn inspect_existing_destination(
    workspace_root: &Path,
    destination: &Path,
) -> Result<(PathBuf, DestinationState), InitError> {
    let metadata = fs::symlink_metadata(destination).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot inspect project destination {}: {error}",
            destination.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(InitError::InvalidInput(format!(
            "project destination is not a regular directory: {}",
            destination.display()
        )));
    }
    if !destination.starts_with(workspace_root) {
        return Err(InitError::InvalidInput(
            "project destination resolves outside the workspace".to_owned(),
        ));
    }
    if directory_is_empty(destination)? {
        Ok((destination.to_path_buf(), DestinationState::Empty))
    } else {
        Err(InitError::InvalidInput(format!(
            "project destination is not empty: {}",
            destination.display()
        )))
    }
}

fn directory_is_empty(path: &Path) -> Result<bool, InitError> {
    directory_is_empty_except(path, None)
}

fn directory_is_empty_except(
    path: &Path,
    ignored: Option<&Path>,
) -> Result<bool, InitError> {
    let entries = fs::read_dir(path).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot read project destination {}: {error}",
            path.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot inspect project destination {}: {error}",
                path.display()
            ))
        })?;
        if ignored.is_none_or(|ignored| entry.path() != ignored) {
            return Ok(false);
        }
    }
    Ok(true)
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

fn create_stage(plan: &InitPlan) -> Result<PathBuf, InitError> {
    let parent = if plan.expected_state == DestinationState::Empty
        && plan.destination == plan.workspace_root
    {
        plan.destination.as_path()
    } else {
        plan.destination.parent().ok_or_else(|| {
            InitError::InvalidInput(
                "project destination has no parent directory".to_owned(),
            )
        })?
    };
    for _ in 0..32 {
        let nonce = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let stage =
            parent.join(format!(".vibra-init-{}-{nonce}.tmp", std::process::id()));
        match fs::create_dir(&stage) {
            Ok(()) => {
                if let Err(error) = populate_stage(plan, &stage) {
                    let _ = fs::remove_dir_all(&stage);
                    return Err(error);
                }
                return Ok(stage);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(InitError::OperationalFailure(format!(
                    "cannot create initialization staging directory {}: {error}",
                    stage.display()
                )));
            }
        }
    }
    Err(InitError::OperationalFailure(
        "cannot allocate a unique initialization staging directory".to_owned(),
    ))
}

fn populate_stage(plan: &InitPlan, stage: &Path) -> Result<(), InitError> {
    let source = stage.join(&plan.source_path);
    let source_parent = source.parent().ok_or_else(|| {
        InitError::OperationalFailure("generated source has no parent".to_owned())
    })?;
    fs::create_dir_all(source_parent).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot prepare staged source directory {}: {error}",
            source_parent.display()
        ))
    })?;
    fs::create_dir(stage.join("tests")).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot prepare staged tests directory: {error}"
        ))
    })?;
    write_new_file(&stage.join("project.vibon"), &plan.project_text)?;
    write_new_file(&source, &plan.source_text)?;

    let snapshot = WorkspaceSnapshot::load_confined(stage).map_err(|error| {
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

fn install_stage(plan: &InitPlan, stage: &Path) -> Result<(), InitError> {
    recheck_destination(plan, stage)?;
    if plan.expected_state == DestinationState::Absent {
        fs::rename(stage, &plan.destination).map_err(|error| {
            InitError::InvalidInput(format!(
                "project destination became unavailable or conflicting: {error}"
            ))
        })?;
        return Ok(());
    }

    let staged_source_root = stage.join("src");
    let staged_tests_root = stage.join("tests");
    let staged_project = stage.join("project.vibon");
    let destination_source_root = plan.destination.join("src");
    let destination_tests_root = plan.destination.join("tests");
    let destination_project = plan.destination.join("project.vibon");
    let mut installed = Vec::new();
    for (source, destination) in [
        (&staged_source_root, &destination_source_root),
        (&staged_tests_root, &destination_tests_root),
    ] {
        if let Err(error) = fs::rename(source, destination) {
            rollback_installed(&installed);
            return Err(InitError::OperationalFailure(format!(
                "cannot install project directory {}: {error}",
                destination.display()
            )));
        }
        installed.push(destination.to_path_buf());
    }
    if let Err(error) = fs::rename(&staged_project, &destination_project) {
        rollback_installed(&installed);
        return Err(InitError::OperationalFailure(format!(
            "cannot install project manifest {}: {error}",
            destination_project.display()
        )));
    }
    let _ = fs::remove_dir_all(stage);
    Ok(())
}

fn rollback_installed(paths: &[PathBuf]) {
    for path in paths.iter().rev() {
        let _ = fs::remove_dir(path);
    }
}

fn recheck_destination(plan: &InitPlan, stage: &Path) -> Result<(), InitError> {
    let current_root = fs::canonicalize(&plan.workspace_root).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot recheck workspace root {}: {error}",
            plan.workspace_root.display()
        ))
    })?;
    if current_root != plan.workspace_root {
        return Err(InitError::InvalidInput(
            "workspace root changed after project initialization was planned"
                .to_owned(),
        ));
    }
    match (plan.expected_state, fs::symlink_metadata(&plan.destination)) {
        (DestinationState::Absent, Err(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(())
        }
        (DestinationState::Absent, Ok(_)) => Err(InitError::InvalidInput(format!(
            "project destination was created after planning: {}",
            plan.destination.display()
        ))),
        (DestinationState::Empty, Ok(metadata))
            if metadata.is_dir() && !metadata.file_type().is_symlink() =>
        {
            let stage_to_ignore =
                (stage.parent() == Some(plan.destination.as_path())).then_some(stage);
            if directory_is_empty_except(&plan.destination, stage_to_ignore)? {
                Ok(())
            } else {
                Err(InitError::InvalidInput(format!(
                    "project destination became nonempty after planning: {}",
                    plan.destination.display()
                )))
            }
        }
        (_, Err(error)) => Err(InitError::InvalidInput(format!(
            "project destination changed after planning: {error}"
        ))),
        (_, Ok(_)) => Err(InitError::InvalidInput(
            "project destination changed after planning".to_owned(),
        )),
    }
}

fn write_new_file(path: &Path, contents: &str) -> Result<(), InitError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot create staged file {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(contents.as_bytes()).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot write staged file {}: {error}",
            path.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot flush staged file {}: {error}",
            path.display()
        ))
    })
}
