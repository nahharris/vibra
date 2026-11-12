use super::*;

pub(in crate::codebase) fn validate_store_path(
    workspace_root: &Path,
    store_relative: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf, OsString), StoreError> {
    if !store_relative.is_relative() || store_relative.as_os_str().is_empty() {
        return Err(StoreError::UnsafePath(format!(
            "store path must be workspace-relative: {}",
            store_relative.display()
        )));
    }
    if store_relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StoreError::UnsafePath(format!(
            "store path contains a non-normal component: {}",
            store_relative.display()
        )));
    }
    let file_name =
        store_relative
            .file_name()
            .map(OsString::from)
            .ok_or_else(|| {
                StoreError::UnsafePath("store path has no file name".to_owned())
            })?;
    let parent_relative = store_relative.parent().unwrap_or_else(|| Path::new(""));
    let root = if workspace_root.is_absolute() {
        workspace_root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                map_path_error(workspace_root, error, "cannot resolve workspace root")
            })?
            .join(workspace_root)
    };
    if !root.is_absolute() {
        return Err(StoreError::UnsafePath(
            "workspace root must resolve to an absolute path".to_owned(),
        ));
    }
    Ok((
        root,
        store_relative.to_path_buf(),
        parent_relative.to_path_buf(),
        file_name,
    ))
}

pub(in crate::codebase) fn temporary_file_name(purpose: &str) -> OsString {
    let serial = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    OsString::from(format!(
        ".vibra-{purpose}-{}-{serial}.tmp",
        std::process::id()
    ))
}

pub(in crate::codebase) fn temporary_relative(
    parent: &Path,
    name: &OsString,
) -> PathBuf {
    if parent.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        parent.join(name)
    }
}

pub(in crate::codebase) fn sync_directory_if_supported(
    directory: &ConfinedDir,
) -> io::Result<()> {
    #[cfg(any(unix, windows))]
    {
        directory.sync_all()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = directory;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory synchronization is unavailable",
        ))
    }
}

pub(in crate::codebase) fn published_sync_error(
    path: &Path,
    published_item: &str,
    error: io::Error,
) -> StoreError {
    StoreError::Io(format!(
        "{published_item} was published at {} but its parent directory flush failed: {error}",
        path.display()
    ))
}

pub(in crate::codebase) fn cleanup_staged_file(parent: &ConfinedDir, name: &OsString) {
    let _ = parent.remove_file(name);
}

pub(in crate::codebase) fn create_confined_parent(
    root: &ConfinedDir,
    parent_relative: &Path,
) -> Result<ConfinedDir, StoreError> {
    if parent_relative.as_os_str().is_empty() {
        return root.open_dir("").map_err(|error| {
            map_path_error(root.path(), error, "cannot open store parent safely")
        });
    }
    let mut current = root.open_dir("").map_err(|error| {
        map_path_error(root.path(), error, "cannot open store parent safely")
    })?;
    for component in parent_relative.components() {
        let Component::Normal(name) = component else {
            return Err(StoreError::UnsafePath(format!(
                "store parent path contains a non-normal component: {}",
                parent_relative.display()
            )));
        };
        match current.open_dir(Path::new(name)) {
            Ok(child) => current = child,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                current.create_dir(name).map_err(|create_error| {
                    map_path_error(
                        parent_relative,
                        create_error,
                        "cannot create store directory safely",
                    )
                })?;
                current = current.open_dir(Path::new(name)).map_err(|open_error| {
                    map_path_error(
                        parent_relative,
                        open_error,
                        "cannot reopen store directory safely",
                    )
                })?;
            }
            Err(error) => {
                return Err(map_path_error(
                    parent_relative,
                    error,
                    "cannot open store directory safely",
                ));
            }
        }
    }
    Ok(current)
}

pub(in crate::codebase) fn map_path_error(
    path: &Path,
    error: io::Error,
    action: &str,
) -> StoreError {
    if matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
    ) {
        StoreError::UnsafePath(format!("{action} {}: {error}", path.display()))
    } else if error.kind() == io::ErrorKind::NotFound {
        StoreError::NotFound(path.to_path_buf())
    } else {
        StoreError::Io(format!("{action} {}: {error}", path.display()))
    }
}

pub(in crate::codebase) fn map_sqlite_error(error: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(code, _) = &error
        && (code.extended_code == rusqlite::ffi::SQLITE_READONLY_ROLLBACK
            || code.extended_code == rusqlite::ffi::SQLITE_READONLY_RECOVERY)
    {
        return StoreError::Io(format!(
            "SQLite needs rollback-journal recovery, which this read-only operation cannot perform; open the store read-write and run `vibra codebase reindex`, then retry: {error}"
        ));
    }
    if let Some(code) = error.sqlite_error_code() {
        if code == ErrorCode::DatabaseBusy || code == ErrorCode::DatabaseLocked {
            return StoreError::Busy(error.to_string());
        }
        if code == ErrorCode::DatabaseCorrupt || code == ErrorCode::NotADatabase {
            return StoreError::Corrupt(error.to_string());
        }
    }
    if let rusqlite::Error::SqliteFailure(_, Some(message)) = &error
        && (message.contains("no such table") || message.contains("no such column"))
    {
        return StoreError::Corrupt(format!(
            "codebase store schema is incomplete: {message}"
        ));
    }
    StoreError::Io(error.to_string())
}

pub(in crate::codebase) fn map_schema_error(error: rusqlite::Error) -> StoreError {
    match &error {
        rusqlite::Error::QueryReturnedNoRows => StoreError::Corrupt(
            "codebase store is missing required schema metadata".to_owned(),
        ),
        rusqlite::Error::SqliteFailure(_, Some(message))
            if message.contains("no such table")
                || message.contains("no such column") =>
        {
            StoreError::Corrupt(format!(
                "codebase store schema is incomplete: {message}"
            ))
        }
        _ => map_sqlite_error(error),
    }
}
