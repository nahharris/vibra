use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationCheckpoint {
    BeforePublish,
    AfterPublish,
}

impl CodebaseStore {
    /// Creates a standalone consistent backup at a new workspace-relative path.
    pub fn backup(
        &self,
        destination_relative: impl AsRef<Path>,
    ) -> Result<BackupReport, StoreError> {
        self.backup_with_checkpoint(destination_relative, |_| {})
    }

    fn backup_with_checkpoint(
        &self,
        destination_relative: impl AsRef<Path>,
        mut checkpoint: impl FnMut(PublicationCheckpoint),
    ) -> Result<BackupReport, StoreError> {
        let (root, relative, parent_relative, file_name) =
            validate_store_path(&self.workspace_root, destination_relative.as_ref())?;
        let root_dir = ConfinedDir::open(&root).map_err(|error| {
            map_path_error(&root, error, "cannot open workspace safely")
        })?;
        let parent = root_dir.open_dir(&parent_relative).map_err(|error| {
            map_path_error(&relative, error, "cannot open backup parent safely")
        })?;
        if parent.child_exists(&file_name).map_err(|error| {
            map_path_error(&relative, error, "cannot inspect backup destination")
        })? {
            return Err(StoreError::UnsafePath(format!(
                "backup destination already exists: {}",
                relative.display()
            )));
        }
        let temporary_name = temporary_file_name("backup");
        parent
            .write_new_file(&temporary_name, &[], None)
            .map_err(|error| {
                map_path_error(&relative, error, "cannot stage backup file")
            })?;
        let temporary_path =
            parent.path_for_child(&temporary_name).map_err(|error| {
                map_path_error(&relative, error, "cannot address staged backup")
            })?;
        let result = (|| {
            let staged_guard = parent
                .open_regular_file_write(&temporary_name)
                .map_err(|error| {
                    map_path_error(&relative, error, "cannot hold staged backup safely")
                })?;
            let source = self.connect()?;
            let mut destination = Connection::open_with_flags(
                &temporary_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(map_sqlite_error)?;
            destination
                .busy_timeout(BUSY_TIMEOUT)
                .map_err(map_sqlite_error)?;
            destination
                .pragma_update(None, "synchronous", "FULL")
                .map_err(map_sqlite_error)?;
            {
                let backup = rusqlite::backup::Backup::new(&source, &mut destination)
                    .map_err(map_sqlite_error)?;
                backup
                    .run_to_completion(64, Duration::from_millis(10), None)
                    .map_err(map_sqlite_error)?;
            }
            destination
                .close()
                .map_err(|(_, error)| map_sqlite_error(error))?;
            let staged = CodebaseStore::open(
                &root,
                temporary_relative(&parent_relative, &temporary_name),
                StoreAccess::ReadOnly,
            )?;
            let report = staged.verify()?;
            drop(staged);
            staged_guard.sync_all().map_err(|error| {
                map_path_error(&relative, error, "cannot flush backup file")
            })?;
            drop(staged_guard);
            sync_directory_if_supported(&parent).map_err(|error| {
                map_path_error(&relative, error, "cannot flush backup parent")
            })?;
            checkpoint(PublicationCheckpoint::BeforePublish);
            parent
                .rename_to_new(&temporary_name, &parent, &file_name)
                .map_err(|error| {
                    map_path_error(&relative, error, "cannot publish backup file")
                })?;
            checkpoint(PublicationCheckpoint::AfterPublish);
            sync_directory_if_supported(&parent).map_err(|error| {
                published_sync_error(&relative, "backup file", error)
            })?;
            Ok(BackupReport {
                destination: relative.clone(),
                revision_count: report.revision_count,
            })
        })();
        if result.is_err() {
            cleanup_staged_file(&parent, &temporary_name);
        }
        result
    }

    /// Restores a verified standalone backup into a new workspace-relative store.
    pub fn restore(
        workspace_root: impl AsRef<Path>,
        source_relative: impl AsRef<Path>,
        store_relative: impl AsRef<Path>,
    ) -> Result<RestoreReport, StoreError> {
        Self::restore_with_checkpoint(
            workspace_root,
            source_relative,
            store_relative,
            |_| {},
        )
    }

    fn restore_with_checkpoint(
        workspace_root: impl AsRef<Path>,
        source_relative: impl AsRef<Path>,
        store_relative: impl AsRef<Path>,
        mut checkpoint: impl FnMut(PublicationCheckpoint),
    ) -> Result<RestoreReport, StoreError> {
        let (root, destination, parent_relative, file_name) =
            validate_store_path(workspace_root.as_ref(), store_relative.as_ref())?;
        let (source_root, source, source_parent_relative, source_name) =
            validate_store_path(workspace_root.as_ref(), source_relative.as_ref())?;
        if root != source_root {
            return Err(StoreError::UnsafePath(
                "restore source and destination roots differ".to_owned(),
            ));
        }
        let root_dir = ConfinedDir::open(&root).map_err(|error| {
            map_path_error(&root, error, "cannot open workspace safely")
        })?;
        let source_parent =
            root_dir
                .open_dir(&source_parent_relative)
                .map_err(|error| {
                    map_path_error(
                        &source,
                        error,
                        "cannot open restore source parent safely",
                    )
                })?;
        let _source_guard =
            source_parent
                .open_regular_file(&source_name)
                .map_err(|error| {
                    map_path_error(&source, error, "cannot open restore source safely")
                })?;
        let source_path =
            source_parent
                .path_for_child(&source_name)
                .map_err(|error| {
                    map_path_error(&source, error, "cannot address restore source")
                })?;
        let _source_store = CodebaseStore::open(&root, &source, StoreAccess::ReadOnly)?;
        let parent = if destination == Path::new(".vibra/codebase.sqlite") {
            create_confined_parent(&root_dir, &parent_relative)?
        } else {
            root_dir.open_dir(&parent_relative).map_err(|error| {
                map_path_error(
                    &destination,
                    error,
                    "restore destination parent must already exist",
                )
            })?
        };
        if parent.child_exists(&file_name).map_err(|error| {
            map_path_error(&destination, error, "cannot inspect restore destination")
        })? {
            return Err(StoreError::UnsafePath(format!(
                "restore destination already exists: {}",
                destination.display()
            )));
        }
        let temporary_name = temporary_file_name("restore");
        parent
            .write_new_file(&temporary_name, &[], None)
            .map_err(|error| {
                map_path_error(&destination, error, "cannot stage restored store")
            })?;
        let temporary_path =
            parent.path_for_child(&temporary_name).map_err(|error| {
                map_path_error(&destination, error, "cannot address staged restore")
            })?;
        let result = (|| {
            let staged_guard = parent
                .open_regular_file_write(&temporary_name)
                .map_err(|error| {
                    map_path_error(
                        &destination,
                        error,
                        "cannot hold staged restore safely",
                    )
                })?;
            let source = Connection::open_with_flags(
                source_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(map_sqlite_error)?;
            let mut target = Connection::open_with_flags(
                &temporary_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(map_sqlite_error)?;
            target
                .busy_timeout(BUSY_TIMEOUT)
                .map_err(map_sqlite_error)?;
            target
                .pragma_update(None, "synchronous", "FULL")
                .map_err(map_sqlite_error)?;
            {
                let backup = rusqlite::backup::Backup::new(&source, &mut target)
                    .map_err(map_sqlite_error)?;
                backup
                    .run_to_completion(64, Duration::from_millis(10), None)
                    .map_err(map_sqlite_error)?;
            }
            target
                .close()
                .map_err(|(_, error)| map_sqlite_error(error))?;
            let temp_relative = temporary_relative(&parent_relative, &temporary_name);
            let staged =
                CodebaseStore::open(&root, &temp_relative, StoreAccess::ReadOnly)?;
            let verified = staged.verify()?;
            drop(staged);
            staged_guard.sync_all().map_err(|error| {
                map_path_error(&destination, error, "cannot flush restored store")
            })?;
            drop(staged_guard);
            sync_directory_if_supported(&parent).map_err(|error| {
                map_path_error(&destination, error, "cannot flush restore parent")
            })?;
            checkpoint(PublicationCheckpoint::BeforePublish);
            parent
                .rename_to_new(&temporary_name, &parent, &file_name)
                .map_err(|error| {
                    map_path_error(&destination, error, "cannot publish restored store")
                })?;
            checkpoint(PublicationCheckpoint::AfterPublish);
            sync_directory_if_supported(&parent).map_err(|error| {
                published_sync_error(&destination, "restored store", error)
            })?;
            Ok(RestoreReport {
                store: destination.clone(),
                revision_count: verified.revision_count,
                object_count: verified.object_count,
                source_count: verified.source_count,
            })
        })();
        if result.is_err() {
            cleanup_staged_file(&parent, &temporary_name);
        }
        result
    }
}

#[cfg(test)]
mod tests;
