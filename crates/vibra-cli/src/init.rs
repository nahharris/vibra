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
    let cleanup = cleanup_stage(stage, plan);
    match (result, cleanup) {
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(InitError::OperationalFailure(format!(
                "{error}; initialization staging cleanup also failed: {cleanup_error}"
            )))
        }
        (Ok(()), Ok(())) => Ok(()),
    }
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
        return Err(InitError::OperationalFailure(
            "workspace-root initialization must use the confined workspace handle"
                .to_owned(),
        ));
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
    let parent = if plan.relative_destination.as_os_str().is_empty() {
        ConfinedDir::open(&plan.workspace_root).map_err(|error| {
            InitError::InvalidInput(format!(
                "cannot safely open workspace for initialization staging: {error}"
            ))
        })?
    } else {
        destination_parent(plan)?.0
    };
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
                    if let Err(cleanup_error) = cleanup_stage(stage, plan) {
                        return Err(InitError::OperationalFailure(format!(
                            "{error}; initialization staging cleanup also failed: {cleanup_error}"
                        )));
                    }
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
    install_stage_with_creation_hook(plan, stage, before_publish, |_| Ok(()))
}

fn install_stage_with_creation_hook(
    plan: &InitPlan,
    stage: &InitStage,
    before_publish: impl FnOnce() -> Result<(), InitError>,
    mut after_create: impl FnMut(usize) -> Result<(), InitError>,
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

    if plan.relative_destination.as_os_str().is_empty() {
        before_publish()?;
        let destination = ConfinedDir::open(&plan.workspace_root).map_err(|error| {
            InitError::InvalidInput(format!(
                "cannot safely reopen workspace before initialization: {error}"
            ))
        })?;
        if !destination.same_as(&stage.parent).map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot verify the current workspace identity: {error}"
            ))
        })? {
            return Err(InitError::InvalidInput(
                "workspace root changed after initialization planning".to_owned(),
            ));
        }
        if !destination.is_empty_except(&stage.name).map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot recheck the current workspace before initialization: {error}"
            ))
        })? {
            return Err(InitError::InvalidInput(format!(
                "project destination became nonempty after planning: {}",
                plan.destination.display()
            )));
        }
        return publish_staged_tree(plan, &destination, &mut after_create);
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
    before_publish()?;

    let created_destination = plan.expected_state == DestinationState::Absent;
    if created_destination {
        destination_parent
            .create_dir(&destination_name)
            .map_err(|error| {
                InitError::InvalidInput(format!(
                    "project destination became unavailable or conflicting: {error}"
                ))
            })?;
    }
    let destination = open_created_destination(
        &destination_parent,
        &destination_name,
        created_destination,
        |parent, name| parent.open_dir(name),
        |directory| directory.is_empty(),
    )?;

    match publish_staged_tree(plan, &destination, &mut after_create) {
        Ok(()) => Ok(()),
        Err(error) if !created_destination => Err(error),
        Err(error) => {
            drop(destination);
            match destination_parent.remove_dir(&destination_name) {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(InitError::OperationalFailure(format!(
                    "{error}; empty project destination rollback failed: {cleanup_error}"
                ))),
            }
        }
    }
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

fn open_created_destination(
    parent: &ConfinedDir,
    name: &OsStr,
    created_destination: bool,
    open: impl FnOnce(&ConfinedDir, &OsStr) -> std::io::Result<ConfinedDir>,
    is_empty: impl FnOnce(&ConfinedDir) -> std::io::Result<bool>,
) -> Result<ConfinedDir, InitError> {
    let destination = match open(parent, name) {
        Ok(destination) => destination,
        Err(error) => {
            let failure = InitError::OperationalFailure(format!(
                "cannot safely open project destination for initialization: {error}"
            ));
            return rollback_new_empty_destination(
                parent,
                name,
                created_destination,
                failure,
            );
        }
    };
    let destination_is_empty = match is_empty(&destination) {
        Ok(is_empty) => is_empty,
        Err(error) => {
            drop(destination);
            let failure = InitError::OperationalFailure(format!(
                "cannot verify project destination before publication: {error}"
            ));
            return rollback_new_empty_destination(
                parent,
                name,
                created_destination,
                failure,
            );
        }
    };
    if !destination_is_empty {
        return Err(InitError::InvalidInput(
            "project destination became nonempty after planning".to_owned(),
        ));
    }
    Ok(destination)
}

fn rollback_new_empty_destination(
    parent: &ConfinedDir,
    name: &OsStr,
    created_destination: bool,
    failure: InitError,
) -> Result<ConfinedDir, InitError> {
    if !created_destination {
        return Err(failure);
    }
    match remove_destination_if_empty(parent, name) {
        Ok(()) => Err(failure),
        Err(cleanup_error) => Err(InitError::OperationalFailure(format!(
            "{failure}; cannot clean up the new empty project destination: {cleanup_error}"
        ))),
    }
}

fn remove_destination_if_empty(
    parent: &ConfinedDir,
    name: &OsStr,
) -> std::io::Result<()> {
    let destination = match parent.open_dir(name) {
        Ok(destination) => destination,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let is_empty = destination.is_empty();
    drop(destination);
    match is_empty {
        Ok(false) => Ok(()),
        Ok(true) => match parent.remove_dir(name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn publish_staged_tree(
    plan: &InitPlan,
    destination: &ConfinedDir,
    after_create: &mut impl FnMut(usize) -> Result<(), InitError>,
) -> Result<(), InitError> {
    let mut journal = PublishJournal::default();
    let publish = (|| {
        destination
            .write_new_file(
                OsStr::new("project.vibon"),
                plan.project_text.as_bytes(),
                None,
            )
            .map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot publish project manifest: {error}"
                ))
            })?;
        journal.project_file = true;
        after_create(0)?;

        destination.create_dir(OsStr::new("src")).map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot publish project source root: {error}"
            ))
        })?;
        journal.source_root = true;
        after_create(1)?;

        let source_root = destination.open_dir("src").map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot open published project source root: {error}"
            ))
        })?;
        source_root
            .create_dir(OsStr::new(&plan.package_name))
            .map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot publish package source directory: {error}"
                ))
            })?;
        journal.package_root = true;
        after_create(2)?;

        let package = source_root.open_dir(&plan.package_name).map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot open published package source directory: {error}"
            ))
        })?;
        package
            .write_new_file(OsStr::new("main.vib"), plan.source_text.as_bytes(), None)
            .map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot publish project entry source: {error}"
                ))
            })?;
        journal.entry_source = true;
        drop(package);
        drop(source_root);
        after_create(3)?;

        destination
            .create_dir(OsStr::new("tests"))
            .map_err(|error| {
                InitError::OperationalFailure(format!(
                    "cannot publish project tests directory: {error}"
                ))
            })?;
        journal.tests_root = true;
        after_create(4)
    })();

    match publish {
        Ok(()) => Ok(()),
        Err(error) => match rollback_published_tree(destination, plan, &journal) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(InitError::OperationalFailure(format!(
                "{error}; published project rollback failed: {rollback_error}"
            ))),
        },
    }
}

#[derive(Default)]
struct PublishJournal {
    project_file: bool,
    source_root: bool,
    package_root: bool,
    entry_source: bool,
    tests_root: bool,
}

fn rollback_published_tree(
    destination: &ConfinedDir,
    plan: &InitPlan,
    journal: &PublishJournal,
) -> Result<(), InitError> {
    if journal.tests_root {
        remove_staged_dir(destination, OsStr::new("tests"))?;
    }
    if journal.entry_source {
        let source_root = destination.open_dir("src").map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot reopen project source root during rollback: {error}"
            ))
        })?;
        let package = source_root.open_dir(&plan.package_name).map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot reopen package source directory during rollback: {error}"
            ))
        })?;
        remove_staged_file(&package, OsStr::new("main.vib"))?;
    }
    if journal.package_root {
        let source_root = destination.open_dir("src").map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot reopen project source root during rollback: {error}"
            ))
        })?;
        remove_staged_dir(&source_root, OsStr::new(&plan.package_name))?;
    }
    if journal.source_root {
        remove_staged_dir(destination, OsStr::new("src"))?;
    }
    if journal.project_file {
        remove_staged_file(destination, OsStr::new("project.vibon"))?;
    }
    Ok(())
}

fn cleanup_stage(stage: InitStage, plan: &InitPlan) -> Result<(), InitError> {
    let InitStage {
        parent,
        name,
        directory,
    } = stage;
    let expected_parent = stage_parent(plan)?;
    if !parent.same_as(&expected_parent).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot verify initialization stage parent during cleanup: {error}"
        ))
    })? {
        return Err(InitError::InvalidInput(
            "initialization stage parent moved outside the current workspace"
                .to_owned(),
        ));
    }
    let same_stage = parent
        .is_same_directory(&name, &directory)
        .map_err(|error| {
            InitError::OperationalFailure(format!(
                "cannot verify initialization staging directory during cleanup: {error}"
            ))
        })?;
    if !same_stage {
        return Err(InitError::InvalidInput(
            "initialization staging path changed before cleanup".to_owned(),
        ));
    }

    match directory.open_dir("src") {
        Ok(source_root) => {
            match source_root.open_dir(&plan.package_name) {
                Ok(package) => {
                    remove_file_if_present(&package, OsStr::new("main.vib"))?;
                    drop(package);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(InitError::OperationalFailure(format!(
                        "cannot open staged package directory during cleanup: {error}"
                    )));
                }
            }
            remove_dir_if_present(&source_root, OsStr::new(&plan.package_name))?;
            drop(source_root);
            remove_dir_if_present(&directory, OsStr::new("src"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(InitError::OperationalFailure(format!(
                "cannot open staged source root during cleanup: {error}"
            )));
        }
    }
    remove_dir_if_present(&directory, OsStr::new("tests"))?;
    remove_file_if_present(&directory, OsStr::new("project.vibon"))?;
    drop(directory);
    parent.remove_dir(&name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot remove initialization staging directory: {error}"
        ))
    })
}

fn stage_parent(plan: &InitPlan) -> Result<ConfinedDir, InitError> {
    if plan.relative_destination.as_os_str().is_empty() {
        ConfinedDir::open(&plan.workspace_root).map_err(|error| {
            InitError::InvalidInput(format!(
                "cannot safely reopen workspace before stage cleanup: {error}"
            ))
        })
    } else {
        destination_parent(plan).map(|(parent, _)| parent)
    }
}

fn remove_file_if_present(parent: &ConfinedDir, name: &OsStr) -> Result<(), InitError> {
    match parent.remove_file(name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(InitError::OperationalFailure(format!(
            "cannot remove staged file {}: {error}",
            parent.path().join(name).display()
        ))),
    }
}

fn remove_dir_if_present(parent: &ConfinedDir, name: &OsStr) -> Result<(), InitError> {
    match parent.remove_dir(name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(InitError::OperationalFailure(format!(
            "cannot remove staged directory {}: {error}",
            parent.path().join(name).display()
        ))),
    }
}

fn remove_staged_file(parent: &ConfinedDir, name: &OsStr) -> Result<(), InitError> {
    parent.remove_file(name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot remove published file {} during rollback: {error}",
            parent.path().join(name).display()
        ))
    })
}

fn remove_staged_dir(parent: &ConfinedDir, name: &OsStr) -> Result<(), InitError> {
    parent.remove_dir(name).map_err(|error| {
        InitError::OperationalFailure(format!(
            "cannot remove published directory {} during rollback: {error}",
            parent.path().join(name).display()
        ))
    })
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
        InitError, apply_init, cleanup_stage, create_stage,
        install_stage_with_creation_hook, install_stage_with_hook,
        open_created_destination, plan_init,
    };
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vibra_workspace::confined_fs::ConfinedDir;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempWorkspace(std::path::PathBuf);

    impl TempWorkspace {
        fn new() -> Self {
            let nonce = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vibra-init-unit-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("create workspace");
            Self(fs::canonicalize(path).expect("canonicalize workspace"))
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
        let error = result.expect_err("injected install failure is reported");
        cleanup_stage(stage, &plan).expect("remove the failed staging tree");

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
    fn partial_current_root_publication_rolls_back_created_entries() {
        let root = TempWorkspace::new();
        let original_root = root.0.canonicalize().expect("canonical workspace root");
        let plan = plan_init(&root.0, None)
            .expect("plan initialization into the current empty workspace");
        let stage = create_stage(&plan).expect("prepare initialization staging tree");

        let result = install_stage_with_creation_hook(
            &plan,
            &stage,
            || Ok(()),
            |index| {
                if index == 0 {
                    Err(InitError::OperationalFailure(
                        "injected failure after first published entry".to_owned(),
                    ))
                } else {
                    Ok(())
                }
            },
        );
        cleanup_stage(stage, &plan).expect("remove staged files after rollback");

        assert!(matches!(result, Err(InitError::OperationalFailure(_))));
        assert_eq!(
            root.0
                .canonicalize()
                .expect("workspace root remains in place"),
            original_root
        );
        assert_eq!(
            fs::read_dir(&root.0)
                .expect("read rolled-back workspace")
                .count(),
            0,
            "partial publication is fully removed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn moved_current_root_is_neither_published_into_nor_cleaned() {
        let root = TempWorkspace::new();
        let moved = root.0.with_file_name(format!(
            "vibra-init-moved-root-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let plan = plan_init(&root.0, None)
            .expect("plan initialization into the current empty workspace");
        let stage = create_stage(&plan).expect("prepare initialization staging tree");

        let installation = install_stage_with_hook(&plan, &stage, || {
            fs::rename(&root.0, &moved).expect("move the planned workspace root");
            fs::create_dir(&root.0)
                .expect("replace the workspace path with an empty directory");
            Ok(())
        });
        let cleanup = cleanup_stage(stage, &plan);
        let moved_entry_count = fs::read_dir(&moved)
            .expect("read the detached workspace")
            .count();
        let published_into_moved_root = moved.join("project.vibon").exists();
        fs::remove_dir_all(&moved).expect("remove detached test workspace");

        assert!(installation.is_err(), "moved workspace must be refused");
        assert!(cleanup.is_err(), "detached stage must be left untouched");
        assert_eq!(moved_entry_count, 1, "only the unmodified stage remains");
        assert!(
            !published_into_moved_root,
            "project files must not be written into the moved workspace"
        );
        assert_eq!(
            fs::read_dir(&root.0)
                .expect("read replacement workspace")
                .count(),
            0,
            "replacement workspace remains untouched"
        );
    }

    #[test]
    fn changed_current_root_identity_is_neither_published_into_nor_cleaned() {
        let workspace = TempWorkspace::new();
        let replacement = TempWorkspace::new();
        let mut plan = plan_init(&workspace.0, None)
            .expect("plan initialization into the current empty workspace");
        let stage = create_stage(&plan).expect("prepare initialization staging tree");
        plan.workspace_root = replacement
            .0
            .canonicalize()
            .expect("canonical replacement workspace");

        let installation = install_stage_with_hook(&plan, &stage, || Ok(()));
        let cleanup = cleanup_stage(stage, &plan);
        let detached_entry_count = fs::read_dir(&workspace.0)
            .expect("read the original workspace directory")
            .count();
        let replacement_entry_count = fs::read_dir(&replacement.0)
            .expect("read the replacement workspace")
            .count();

        assert!(installation.is_err(), "changed workspace must be refused");
        assert!(cleanup.is_err(), "detached stage must be left untouched");
        assert_eq!(detached_entry_count, 1, "only the unmodified stage remains");
        assert_eq!(replacement_entry_count, 0, "replacement remains untouched");
    }

    #[test]
    fn partial_absent_destination_publication_removes_the_new_directory() {
        let root = TempWorkspace::new();
        let destination = root.0.join("demo");
        let plan = plan_init(&root.0, Some(Path::new("demo")))
            .expect("plan initialization into an absent destination");
        let stage = create_stage(&plan).expect("prepare initialization staging tree");

        let result = install_stage_with_creation_hook(
            &plan,
            &stage,
            || Ok(()),
            |index| {
                if index == 0 {
                    Err(InitError::OperationalFailure(
                        "injected failure after first published entry".to_owned(),
                    ))
                } else {
                    Ok(())
                }
            },
        );
        cleanup_stage(stage, &plan).expect("remove staged files after rollback");

        assert!(matches!(result, Err(InitError::OperationalFailure(_))));
        assert!(
            !destination.exists(),
            "failed publication removes its new root"
        );
        assert_eq!(
            fs::read_dir(&root.0)
                .expect("read workspace after rollback")
                .count(),
            0
        );
    }

    #[test]
    fn failed_destination_open_removes_the_new_empty_directory() {
        let root = TempWorkspace::new();
        let destination = root.0.join("demo");
        fs::create_dir(&destination).expect("create destination after planning");
        let parent = ConfinedDir::open(&root.0).expect("open workspace parent");

        let result = open_created_destination(
            &parent,
            OsStr::new("demo"),
            true,
            |_, _| Err(std::io::Error::other("injected destination open failure")),
            |_| unreachable!("emptiness check follows a successful open"),
        );

        assert!(matches!(result, Err(InitError::OperationalFailure(_))));
        assert!(
            !destination.exists(),
            "the empty destination created by this attempt is removed"
        );
    }

    #[test]
    fn failed_destination_empty_check_removes_the_new_empty_directory() {
        let root = TempWorkspace::new();
        let destination = root.0.join("demo");
        fs::create_dir(&destination).expect("create destination after planning");
        let parent = ConfinedDir::open(&root.0).expect("open workspace parent");

        let result = open_created_destination(
            &parent,
            OsStr::new("demo"),
            true,
            |parent, name| parent.open_dir(name),
            |_| Err(std::io::Error::other("injected destination read failure")),
        );

        assert!(matches!(result, Err(InitError::OperationalFailure(_))));
        assert!(
            !destination.exists(),
            "the empty destination created by this attempt is removed"
        );
    }

    #[test]
    fn failed_destination_empty_check_preserves_concurrent_content() {
        let root = TempWorkspace::new();
        let destination = root.0.join("demo");
        fs::create_dir(&destination).expect("create destination after planning");
        let parent = ConfinedDir::open(&root.0).expect("open workspace parent");

        let result = open_created_destination(
            &parent,
            OsStr::new("demo"),
            true,
            |parent, name| parent.open_dir(name),
            |directory| {
                fs::write(directory.path().join("concurrent.txt"), b"keep")?;
                Err(std::io::Error::other("injected destination read failure"))
            },
        );

        assert!(matches!(result, Err(InitError::OperationalFailure(_))));
        assert_eq!(
            fs::read(destination.join("concurrent.txt"))
                .expect("preserve content created concurrently"),
            b"keep"
        );
    }

    #[test]
    fn apply_init_wrapper_still_installs_a_prepared_project() {
        let root = TempWorkspace::new();
        let plan = plan_init(&root.0, Some(Path::new("demo")))
            .expect("plan initialization into an absent destination");

        apply_init(&plan).expect("publish staged project with rollback on failure");

        assert!(root.0.join("demo/project.vibon").is_file());
        assert!(root.0.join("demo/src/demo/main.vib").is_file());
        assert!(root.0.join("demo/tests").is_dir());
    }
}
