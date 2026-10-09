use nebulabook::import_export::{export_backup, import_bytes, import_file, ImportMode};
use nebulabook::model::Notebook;
use nebulabook::nebula_format;
use nebulabook::storage::Storage;

#[test]
fn native_import_edit_trash_backup_and_reopen_are_non_destructive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notebook.nebula");
    let mut source = Notebook::default();
    import_bytes(
        "原始笔记.html",
        "<h1>重要内容</h1><script>alert('never run')</script><p>第二行</p>".as_bytes(),
        &mut source,
        ImportMode::Copy,
    )
    .unwrap();
    let source_id = source.notes[0].id.clone();
    source.notes[0].set_pinned(true);
    let native = nebula_format::encode(&source).unwrap();
    let (mut storage, mut notebook) = Storage::open(path.clone()).unwrap();
    let report = import_bytes("source.nebula", &native, &mut notebook, ImportMode::Copy).unwrap();
    assert_eq!(report.added, 1);
    assert_ne!(notebook.notes[0].id, source_id);
    assert!(notebook.notes[0].is_pinned);
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
    let backup = directory.path().join("export.nebula");
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
    let mut corrupt = nebula_format::encode(&original).unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(import_bytes("bad.nebula", &corrupt, &mut notebook, ImportMode::Copy).is_err());
    assert_eq!(notebook, original);
    import_bytes("same.txt", b"new copy", &mut notebook, ImportMode::Copy).unwrap();
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0], original.notes[0]);
    assert_ne!(notebook.notes[0].id, notebook.notes[1].id);
}
