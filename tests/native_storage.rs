use fs2::FileExt;
use nebulabook::import_export::{export_backup, import_bytes, import_file, ImportMode};
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
fn native_round_trip_preserves_all_current_schema_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let notebook = rich_notebook();
    let (mut storage, _) = Storage::open(path.clone()).unwrap();
    storage.save(&notebook).unwrap();
    drop(storage);
    assert_eq!(Storage::open(path).unwrap().1, notebook);
}

#[test]
fn old_json_and_backup_are_neither_read_nor_changed() {
    for old_bytes in [
        b"invalid old JSON".to_vec(),
        serde_json::to_vec(&rich_notebook()).unwrap(),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notebook.nebula");
        let json = path.with_extension("json");
        let backup = path.with_extension("json.bak");
        fs::write(&json, &old_bytes).unwrap();
        fs::write(&backup, b"old backup").unwrap();
        let (mut storage, notebook) = Storage::open(path.clone()).unwrap();
        assert_eq!(notebook, Notebook::default());
        assert!(!path.exists());
        storage.save(&notebook).unwrap();
        assert_eq!(fs::read(&json).unwrap(), old_bytes);
        assert_eq!(fs::read(backup).unwrap(), b"old backup");
        assert!(!path.with_extension("migration.nebula").exists());
        // Changes to an unrelated old file no longer block native operations.
        fs::write(&json, b"changed old JSON").unwrap();
        let notebook = rich_notebook();
        storage.save(&notebook).unwrap();
        drop(storage);
        assert_eq!(Storage::open(path).unwrap().1, notebook);
        assert_eq!(fs::read(json).unwrap(), b"changed old JSON");
    }
}

#[test]
fn json_path_is_rejected_without_creating_native_files() {
    let directory = tempfile::tempdir().unwrap();
    for extension in ["json", "JSON", "NEBULA", "txt"] {
        let source = directory.path().join(format!("notebook.{extension}"));
        fs::write(&source, b"untouched").unwrap();
        assert!(matches!(
            Storage::open(source.clone()),
            Err(StorageError::InvalidData { .. })
        ));
        assert_eq!(fs::read(&source).unwrap(), b"untouched");
        assert!(!source.with_extension("nebula").exists());
    }
}

#[test]
fn native_file_does_not_inspect_adjacent_json_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    // A directory cannot be read as a JSON snapshot; native operations ignore it.
    fs::create_dir(path.with_extension("json")).unwrap();
    let (mut storage, _) = Storage::open(path.clone()).unwrap();
    let notebook = rich_notebook();
    storage.save(&notebook).unwrap();
    drop(storage);
    assert_eq!(Storage::open(path).unwrap().1, notebook);
}

#[test]
fn corrupt_native_never_falls_back_to_json() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let old = serde_json::to_vec(&rich_notebook()).unwrap();
    fs::write(path.with_extension("json"), &old).unwrap();
    for bad in [
        b"modified native".to_vec(),
        serde_json::to_vec(&Notebook::default()).unwrap(),
    ] {
        fs::write(&path, &bad).unwrap();
        assert!(matches!(
            Storage::open(path.clone()),
            Err(StorageError::InvalidData { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), bad);
        assert_eq!(fs::read(path.with_extension("json")).unwrap(), old);
    }
}

#[test]
fn missing_primary_with_any_native_recovery_snapshot_requires_explicit_recovery() {
    for extension in ["backup.nebula", "migration.nebula"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notebook.nebula");
        let recovery = path.with_extension(extension);
        let bytes = nebula_format::encode(&rich_notebook()).unwrap();
        fs::write(&recovery, &bytes).unwrap();
        assert!(matches!(
            Storage::open(path.clone()),
            Err(StorageError::RecoveryAvailable { .. })
        ));
        assert!(!path.exists());
        assert_eq!(fs::read(recovery).unwrap(), bytes);
    }
}

#[test]
fn current_format_writers_keep_the_established_operating_system_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path.with_extension("json.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        Storage::open(path.clone()),
        Err(StorageError::Locked(_))
    ));
    drop(lock);
    let (storage, _) = Storage::open(path.clone()).unwrap();
    assert!(matches!(Storage::open(path), Err(StorageError::Locked(_))));
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
fn authenticated_native_import_is_transactional() {
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
    let plaintext = directory.path().join("unsupported.json");
    assert!(export_backup(&plaintext, &notebook).is_err());
    assert!(!plaintext.exists());
    assert!(import_bytes(
        "unsupported.json",
        &serde_json::to_vec(&notebook).unwrap(),
        &mut target,
        ImportMode::Merge
    )
    .is_err());
    assert_eq!(target, notebook);
}

#[test]
fn current_schema_record_and_metadata_limits_remain_enforced() {
    let mut notebook = Notebook {
        legacy_settings: vec![serde_json::Value::Null; 100_001],
        ..Notebook::default()
    };
    assert!(nebula_format::encode(&notebook).is_err());
    let mut value = serde_json::Value::Null;
    for _ in 0..66 {
        value = serde_json::json!([value]);
    }
    notebook.legacy_settings.clear();
    notebook.legacy_archives.push(value);
    assert!(nebula_format::encode(&notebook).is_err());
}
