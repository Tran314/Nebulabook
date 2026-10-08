use nebulabook::import_export::{export_backup, import_bytes, import_file, ImportMode};
use nebulabook::model::Notebook;
use nebulabook::storage::Storage;

#[test]
fn legacy_migration_edit_trash_backup_and_reopen_are_non_destructive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.json");
    let legacy = serde_json::json!({
        "notes": [{
            "id": "old-note", "userId": "anonymous-user", "folderId": null,
            "title": "旧笔记", "content": "<h1>重要内容</h1><script>alert('never run')</script><p>第二行</p>",
            "isPinned": true, "isDeleted": false, "deletedAt": null,
            "version": 4, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
            "tags": [], "syncedAt": null
        }], "folders": [], "tags": [], "settings": []
    });
    let (mut storage, mut notebook) = Storage::open(path.clone()).unwrap();
    let report = import_bytes(
        "old.json",
        legacy.to_string().as_bytes(),
        &mut notebook,
        ImportMode::Copy,
    )
    .unwrap();
    assert_eq!(report.added, 1);
    assert_ne!(notebook.notes[0].id, "old-note");
    assert!(notebook.notes[0].content.contains("重要内容"));
    assert!(!notebook.notes[0].content.contains("never run"));
    assert!(notebook.notes[0]
        .original_html
        .as_ref()
        .unwrap()
        .contains("<script>"));
    storage.save(&notebook).unwrap();

    notebook.notes[0].update("原生编辑".into(), String::new());
    notebook.notes[0].set_deleted(true);
    storage.save(&notebook).unwrap();
    let backup = directory.path().join("export.json");
    export_backup(&backup, &notebook).unwrap();
    drop(storage);
    let (_, reopened) = Storage::open(path).unwrap();
    assert_eq!(reopened, notebook);
    assert!(reopened.notes[0].content.is_empty());
    assert!(reopened.notes[0].is_deleted);

    let mut recovered = Notebook::default();
    import_file(&backup, &mut recovered, ImportMode::Copy).unwrap();
    assert_eq!(recovered.notes[0].title, "原生编辑");
    assert_eq!(
        recovered.notes[0].original_html,
        notebook.notes[0].original_html
    );
    recovered.notes[0].set_deleted(false);
    assert!(recovered.notes[0].deleted_at.is_none());
}

#[test]
fn a_failed_import_and_duplicate_import_cannot_replace_current_content() {
    let mut notebook = Notebook::default();
    import_bytes("same.txt", b"original", &mut notebook, ImportMode::Copy).unwrap();
    let original = notebook.clone();
    assert!(import_bytes(
        "bad.json",
        b"{\"schema_version\":99}",
        &mut notebook,
        ImportMode::Copy
    )
    .is_err());
    assert_eq!(notebook, original);
    import_bytes("same.txt", b"new copy", &mut notebook, ImportMode::Copy).unwrap();
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0], original.notes[0]);
    assert_ne!(notebook.notes[0].id, notebook.notes[1].id);
}
