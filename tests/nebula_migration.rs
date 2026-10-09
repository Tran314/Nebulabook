use fs2::FileExt;
use nebulabook::import_export::{
    export_backup, export_plaintext_json, import_bytes, import_file, ImportMode,
};
use nebulabook::model::{Folder, Note, Notebook, Tag};
use nebulabook::nebula_format;
use nebulabook::storage::{Storage, StorageError};
use std::fs;

fn rich_notebook() -> Notebook {
    let mut notebook = Notebook::default();
    notebook.folders.push(Folder {
        id: "folder-1".into(),
        name: "项目".into(),
        parent_id: None,
        sort_order: 7,
    });
    notebook.tags.push(Tag {
        id: "tag-1".into(),
        name: "保留".into(),
        color: "#abc123".into(),
    });
    let mut note = Note::new(
        "原生 4.1 📝".into(),
        "第一行\n<script>inert</script>".into(),
    );
    note.folder_id = Some("folder-1".into());
    note.tag_ids.push("tag-1".into());
    note.original_html = Some("<b onclick='never()'>原文</b>".into());
    note.legacy_metadata = Some(serde_json::json!({"custom": [1, {"nested": "keep"}]}));
    note.set_pinned(true);
    note.set_deleted(true);
    notebook.notes.push(note);
    notebook
        .legacy_settings
        .push(serde_json::json!({"custom-setting":true}));
    notebook
        .legacy_archives
        .push(serde_json::json!({"unknown_old_field":"preserved"}));
    notebook
}

#[test]
fn native_json_migration_preserves_every_field_source_and_old_backup() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("notebook.json");
    let old_backup = directory.path().join("notebook.json.bak");
    let notebook = rich_notebook();
    let bytes = serde_json::to_vec_pretty(&notebook).unwrap();
    fs::write(&source, &bytes).unwrap();
    fs::write(&old_backup, b"old recovery bytes").unwrap();
    let (storage, migrated) = Storage::open(source.clone()).unwrap();
    assert_eq!(storage.path(), source.with_extension("nebula"));
    assert_eq!(migrated, notebook);
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::read(old_backup).unwrap(), b"old recovery bytes");
    assert_eq!(
        nebula_format::decode(&fs::read(storage.path()).unwrap()).unwrap(),
        notebook
    );
    let recovery = source.with_extension("migration.nebula");
    assert_eq!(
        fs::read(recovery).unwrap(),
        fs::read(storage.path()).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            storage.path().to_path_buf(),
            source.with_extension("migration.nebula"),
            directory.path().join("notebook.json.lock"),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn current_nebula_wins_over_unchanged_stale_json() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    fs::write(&old, serde_json::to_vec(&rich_notebook()).unwrap()).unwrap();
    let (mut storage, mut notebook) = Storage::open(old.clone()).unwrap();
    notebook.notes[0].update("new version".into(), "current content".into());
    storage.save(&notebook).unwrap();
    drop(storage);
    let (_, reopened) = Storage::open(old).unwrap();
    assert_eq!(reopened, notebook);
}

#[test]
fn downgrade_modification_blocks_open_and_save_without_overwriting_either_copy() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    fs::write(&old, serde_json::to_vec(&rich_notebook()).unwrap()).unwrap();
    let (mut storage, mut notebook) = Storage::open(old.clone()).unwrap();
    let native = fs::read(storage.path()).unwrap();
    let changed = serde_json::to_vec(&Notebook::default()).unwrap();
    fs::write(&old, &changed).unwrap();
    notebook.notes[0].update("unsaved draft".into(), "keep in memory".into());
    assert!(matches!(
        storage.save(&notebook),
        Err(StorageError::LegacyConflict(_))
    ));
    assert_eq!(notebook.notes[0].content, "keep in memory");
    assert_eq!(fs::read(storage.path()).unwrap(), native);
    drop(storage);
    assert!(matches!(
        Storage::open(old.clone()),
        Err(StorageError::LegacyConflict(_))
    ));
    assert_eq!(fs::read(old).unwrap(), changed);
}

#[test]
fn removing_preserved_plaintext_is_allowed_but_changed_reappearance_is_not() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    fs::write(&old, serde_json::to_vec(&rich_notebook()).unwrap()).unwrap();
    let (mut storage, mut notebook) = Storage::open(old.clone()).unwrap();
    fs::remove_file(&old).unwrap();
    notebook.notes[0].update("after removal".into(), "saved".into());
    storage.save(&notebook).unwrap();
    drop(storage);
    let (storage, read) = Storage::open(old.clone()).unwrap();
    assert_eq!(read, notebook);
    drop(storage);
    fs::write(&old, b"new downgraded file").unwrap();
    assert!(matches!(
        Storage::open(old),
        Err(StorageError::LegacyConflict(_))
    ));
}

#[test]
fn unexpected_json_beside_unmigrated_native_file_is_a_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let (mut storage, notebook) = Storage::open(path.clone()).unwrap();
    storage.save(&notebook).unwrap();
    let bytes = fs::read(&path).unwrap();
    drop(storage);
    fs::write(path.with_extension("json"), b"{}").unwrap();
    assert!(matches!(
        Storage::open(path.clone()),
        Err(StorageError::LegacyConflict(_))
    ));
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn corrupt_native_never_falls_back_to_valid_stale_json() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    fs::write(&old, serde_json::to_vec(&rich_notebook()).unwrap()).unwrap();
    let (storage, _) = Storage::open(old.clone()).unwrap();
    let path = storage.path().to_owned();
    drop(storage);
    for bad in [
        b"modified native".to_vec(),
        serde_json::to_vec(&Notebook::default()).unwrap(),
    ] {
        fs::write(&path, &bad).unwrap();
        assert!(matches!(
            Storage::open(old.clone()),
            Err(StorageError::InvalidData { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), bad);
    }
}

#[test]
fn corrupt_or_future_legacy_native_is_not_overwritten_or_partly_migrated() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    for bad in [
        "broken",
        r#"{"schema_version":99,"notes":[],"folders":[],"tags":[]}"#,
        r#"{"schema_version":1,"notes":[],"folders":[],"tags":[],"custom_unhandled":true}"#,
        r#"{"schema_version":1,"notes":[],"notes":[],"folders":[],"tags":[]}"#,
    ] {
        fs::write(&old, bad).unwrap();
        assert!(Storage::open(old.clone()).is_err());
        assert_eq!(fs::read(&old).unwrap(), bad.as_bytes());
        assert!(!old.with_extension("nebula").exists());
        assert!(!old.with_extension("migration.nebula").exists());
    }
}

#[test]
fn interrupted_migration_and_missing_primary_require_explicit_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    let original = serde_json::to_vec(&rich_notebook()).unwrap();
    fs::write(&old, &original).unwrap();
    let (storage, _) = Storage::open(old.clone()).unwrap();
    let path = storage.path().to_owned();
    drop(storage);
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        Storage::open(old.clone()),
        Err(StorageError::RecoveryAvailable { .. })
    ));
    assert!(!path.exists());
    assert_eq!(fs::read(old).unwrap(), original);
}

#[test]
fn missing_legacy_with_legacy_backup_does_not_create_empty_native() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("notebook.json");
    fs::write(
        directory.path().join("notebook.json.bak"),
        b"important recovery",
    )
    .unwrap();
    assert!(matches!(
        Storage::open(old.clone()),
        Err(StorageError::RecoveryAvailable { .. })
    ));
    assert!(!old.with_extension("nebula").exists());
}

#[test]
fn old_and_new_versions_share_the_same_operating_system_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(directory.path().join("notebook.json.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        Storage::open(path.clone()),
        Err(StorageError::Locked(_))
    ));
    drop(lock);
    let (storage, _) = Storage::open(path.clone()).unwrap();
    assert!(matches!(
        Storage::open(path.with_extension("json")),
        Err(StorageError::Locked(_))
    ));
    drop(storage);
}

#[test]
fn unchanged_save_preserves_ciphertext_and_recovery_generation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let (mut storage, notebook) = Storage::open(path.clone()).unwrap();
    storage.save(&notebook).unwrap();
    let original = fs::read(&path).unwrap();
    storage.save(&notebook).unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(!storage.backup_path().exists());
}

#[test]
fn encrypted_import_is_transactional_and_plaintext_export_is_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let notebook = rich_notebook();
    let portable = directory.path().join("backup.nebula");
    export_backup(&portable, &notebook).unwrap();
    let raw = fs::read(&portable).unwrap();
    let mut target = Notebook::default();
    import_file(&portable, &mut target, ImportMode::Merge).unwrap();
    assert_eq!(target, notebook);
    for offset in [0, 8, 10, 12, 36, 44, raw.len() - 1] {
        let mut corrupt = raw.clone();
        corrupt[offset] ^= 0x40;
        assert!(import_bytes("bad.nebula", &corrupt, &mut target, ImportMode::Copy).is_err());
        assert_eq!(target, notebook);
    }
    let plaintext = directory.path().join("explicit.json");
    assert!(export_backup(&plaintext, &notebook).is_err());
    assert!(!plaintext.exists());
    export_plaintext_json(&plaintext, &notebook).unwrap();
    assert_eq!(
        serde_json::from_slice::<Notebook>(&fs::read(&plaintext).unwrap()).unwrap(),
        notebook
    );
    let mut restored = Notebook::default();
    import_file(&plaintext, &mut restored, ImportMode::Merge).unwrap();
    assert_eq!(restored, notebook);
}

#[test]
fn native_size_count_and_metadata_structure_limits_do_not_mutate_target() {
    let mut target = rich_notebook();
    let original = target.clone();
    let raw = serde_json::json!({"schema_version":1,"notes":[],"folders":[],"tags":[],"legacy_settings":vec![serde_json::Value::Null; 100_001]});
    assert!(import_bytes(
        "oversized-array.json",
        raw.to_string().as_bytes(),
        &mut target,
        ImportMode::Copy
    )
    .is_err());
    assert_eq!(target, original);
    let mut value = serde_json::Value::Null;
    for _ in 0..66 {
        value = serde_json::json!([value]);
    }
    let mut notebook = Notebook::default();
    notebook.legacy_archives.push(value);
    assert!(nebula_format::encode(&notebook).is_err());
}

#[test]
fn uppercase_legacy_storage_paths_are_rejected_without_ignoring_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("notebook.JSON");
    let original = serde_json::to_vec(&rich_notebook()).unwrap();
    fs::write(&source, &original).unwrap();
    assert!(matches!(
        Storage::open(source.clone()),
        Err(StorageError::InvalidData { .. })
    ));
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!source.with_extension("nebula").exists());
}

#[test]
fn recovery_error_lists_the_initial_migration_snapshot() {
    let error = StorageError::InvalidData {
        path: "notebook.nebula".into(),
        message: "integrity check failed".into(),
    }
    .to_string();
    assert!(error.contains("notebook.backup.nebula"));
    assert!(error.contains("notebook.migration.nebula"));
}
