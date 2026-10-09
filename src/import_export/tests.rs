use super::*;
use serde_json::json;

fn legacy_note(id: &str) -> Value {
    json!({
        "id": id, "userId": "anonymous-user", "folderId": null,
        "title": "旧笔记", "content": "<p>Hello &amp; 你好</p>",
        "isPinned": true, "isDeleted": false, "deletedAt": null,
        "version": 3, "createdAt": "2025-01-01T00:00:00Z",
        "updatedAt": "2025-01-02T00:00:00Z", "tags": [], "syncedAt": null
    })
}

fn import_json(
    value: Value,
    notebook: &mut Notebook,
    mode: ImportMode,
) -> Result<ImportReport, String> {
    import_bytes(
        "backup.json",
        &serde_json::to_vec(&value).unwrap(),
        notebook,
        mode,
    )
}

#[test]
fn plain_text_and_markdown_keep_exact_unicode_content() {
    for name in ["日记.TXT", "日记.md"] {
        let mut notebook = Notebook::default();
        let content = "# 标题\n中文 😀\n\n<script>this is literal text</script>\n";
        import_bytes(
            name,
            content.as_bytes(),
            &mut notebook,
            ImportMode::default(),
        )
        .unwrap();
        assert_eq!(notebook.notes[0].title, "日记");
        assert_eq!(notebook.notes[0].content, content);
        assert_eq!(notebook.notes[0].original_html, None);
    }
}

#[test]
fn html_is_nonexecuting_text_and_original_source_survives() {
    let source = "\u{feff}<p>Hello &amp; 你好</p><script>window.SECRET = 123</script><style>.secret {color:red}</style><img src='https://invalid.example/tracker' onerror='alert(1)'>";
    let mut notebook = Notebook::default();
    import_bytes(
        "old.html",
        source.as_bytes(),
        &mut notebook,
        ImportMode::Copy,
    )
    .unwrap();
    let note = &notebook.notes[0];
    assert!(note.content.contains("Hello & 你好"));
    assert!(!note.content.contains("window.SECRET"));
    assert!(!note.content.contains("alert(1)"));
    assert!(!note.content.contains("color:red"));
    assert_eq!(note.original_html.as_deref(), Some(source));
}

#[test]
fn malformed_json_unknown_fields_and_future_schema_are_nondestructive() {
    let mut notebook = Notebook::default();
    notebook
        .notes
        .push(Note::new("Keep me".into(), "Unsaved edits".into()));
    let original = notebook.clone();
    for source in [
        "{broken",
        "null",
        "42",
        "{}",
        r#"{"schema_version":2,"notes":[],"folders":[],"tags":[]}"#,
        r#"{"schema_version":1,"notes":[],"folders":[],"tags":[],"unknown":"keep"}"#,
        r#"{"notes":[{"id":"incomplete"}]}"#,
    ] {
        assert!(
            import_bytes(
                "bad.json",
                source.as_bytes(),
                &mut notebook,
                ImportMode::Merge
            )
            .is_err(),
            "accepted {source}"
        );
        assert_eq!(notebook, original);
    }
}

#[test]
fn a_later_invalid_record_rolls_back_the_entire_import() {
    let mut notebook = Notebook::default();
    notebook
        .notes
        .push(Note::new("Existing".into(), "Keep".into()));
    let original = notebook.clone();
    let source = json!([legacy_note("valid"), {"id":"bad"}]);
    assert!(import_json(source, &mut notebook, ImportMode::Copy).is_err());
    assert_eq!(notebook, original);
}

#[test]
fn default_copy_imports_duplicate_external_ids_without_overwriting() {
    let source = json!([legacy_note("old-id")]);
    let mut notebook = Notebook::default();
    let first = import_json(source.clone(), &mut notebook, ImportMode::default()).unwrap();
    notebook.notes[0].update("Edited locally".into(), "Important new text".into());
    let keep = notebook.notes[0].clone();
    let second = import_json(source, &mut notebook, ImportMode::default()).unwrap();
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0], keep);
    assert_ne!(notebook.notes[0].id, notebook.notes[1].id);
    assert_ne!(notebook.notes[1].id, "old-id");
    assert_eq!(first.added, 1);
    assert_eq!(second.updated, 0);
}

#[test]
fn duplicate_ids_inside_a_backup_are_rejected_without_partial_changes() {
    let mut notebook = Notebook::default();
    let original = notebook.clone();
    assert!(import_json(
        json!([legacy_note("same"), legacy_note("same")]),
        &mut notebook,
        ImportMode::Copy
    )
    .is_err());
    assert_eq!(notebook, original);
}

#[test]
fn explicit_merge_updates_matching_id_and_keeps_unrelated_notes() {
    let mut notebook = Notebook::default();
    import_json(
        json!([legacy_note("old-id")]),
        &mut notebook,
        ImportMode::Merge,
    )
    .unwrap();
    notebook
        .notes
        .push(Note::new("Unrelated".into(), "Keep".into()));
    let unrelated = notebook.notes[1].clone();
    let mut incoming = legacy_note("old-id");
    incoming["title"] = json!("Explicit replacement");
    let report = import_json(json!([incoming]), &mut notebook, ImportMode::Merge).unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(report.added, 0);
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0].title, "Explicit replacement");
    assert_eq!(notebook.notes[1], unrelated);
}

fn legacy_full_export() -> Value {
    let mut note = legacy_note("note-old");
    note["folderId"] = json!("folder-child");
    note["tags"] = json!(["tag-old"]);
    let folder = |id: &str, parent: Option<&str>| {
        json!({
            "id": id, "userId": "anonymous-user", "name": id, "parentId": parent,
            "sortOrder": 0, "createdAt": "2025-01-01T00:00:00Z", "updatedAt": "2025-01-01T00:00:00Z", "syncedAt": null
        })
    };
    json!({
        "format": "nebula-legacy-indexeddb", "schemaVersion": 1,
        "exportedAt": "2025-01-03T00:00:00Z", "database": {"name":"NebulaLocalDB", "version":10},
        "stores": {
            "notes": [note],
            "folders": [folder("folder-parent", None), folder("folder-child", Some("folder-parent"))],
            "tags": [{"id":"tag-old", "userId":"anonymous-user", "name":"Work", "color":"#112233", "createdAt":"2025-01-01T00:00:00Z", "syncedAt":null}],
            "settings": [{"userId":"anonymous-user", "theme":"dark", "editorFontSize":16,"autoSave":true,"autoSaveInterval":30,"syncedAt":null}]
        }
    })
}

#[test]
fn copied_legacy_ids_remap_folder_hierarchy_and_tags_together() {
    let source = legacy_full_export();
    let mut notebook = Notebook::default();
    import_json(source.clone(), &mut notebook, ImportMode::Copy).unwrap();
    notebook.validate().unwrap();
    let note = &notebook.notes[0];
    let child = notebook
        .folders
        .iter()
        .find(|folder| folder.name == "folder-child")
        .unwrap();
    let parent = notebook
        .folders
        .iter()
        .find(|folder| folder.name == "folder-parent")
        .unwrap();
    assert_eq!(note.folder_id.as_deref(), Some(child.id.as_str()));
    assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
    assert_ne!(child.id, "folder-child");
    assert_eq!(note.tag_ids, vec![notebook.tags[0].id.clone()]);
    assert_ne!(notebook.tags[0].id, "tag-old");
    assert_eq!(
        note.original_html.as_deref(),
        Some("<p>Hello &amp; 你好</p>")
    );
    assert_eq!(
        note.legacy_metadata.as_ref(),
        Some(&source["stores"]["notes"][0])
    );
    assert_eq!(notebook.legacy_settings[0], source["stores"]["settings"][0]);
    assert_eq!(notebook.legacy_archives, vec![source]);
}

#[test]
fn missing_and_cross_user_references_are_rejected() {
    for bad in ["unavailable", "other-owner"] {
        let mut source = legacy_full_export();
        if bad == "unavailable" {
            source["stores"]["folders"] = json!([]);
        } else {
            source["stores"]["tags"][0]["userId"] = json!("someone-else");
        }
        let mut notebook = Notebook::default();
        assert!(import_json(source, &mut notebook, ImportMode::Copy).is_err());
        assert!(notebook.notes.is_empty());
        assert!(notebook.folders.is_empty());
    }
}

#[test]
fn full_native_backup_round_trips_all_metadata_and_trashed_notes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.nebula");
    let mut notebook = Notebook::default();
    import_json(legacy_full_export(), &mut notebook, ImportMode::Merge).unwrap();
    notebook.notes[0].set_deleted(true);
    export_backup(&path, &notebook).unwrap();
    let mut restored = Notebook::default();
    import_file(&path, &mut restored, ImportMode::Merge).unwrap();
    assert_eq!(restored, notebook);
}

#[test]
fn exports_never_overwrite_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.nebula");
    fs::write(&path, b"important existing file").unwrap();
    assert!(export_backup(&path, &Notebook::default())
        .unwrap_err()
        .contains("already exists"));
    let note = Note::new("Title".into(), "Replacement".into());
    assert!(export_note(&path, &note, NoteFormat::Text).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"important existing file");
}

#[test]
fn text_and_markdown_exports_keep_exact_editor_source() {
    let directory = tempfile::tempdir().unwrap();
    let note = Note::new("Title".into(), "# 标题\n\nLiteral <tags> 😀\n".into());
    for (name, format) in [
        ("note.txt", NoteFormat::Text),
        ("note.md", NoteFormat::Markdown),
    ] {
        let path = directory.path().join(name);
        export_note(&path, &note, format).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), note.content);
    }
}

#[test]
fn oversized_files_bad_utf8_and_unsupported_formats_leave_data_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.txt");
    File::create(&path)
        .unwrap()
        .set_len(MAX_IMPORT_BYTES + 1)
        .unwrap();
    let mut notebook = Notebook::default();
    let original = notebook.clone();
    assert!(import_file(&path, &mut notebook, ImportMode::Copy)
        .unwrap_err()
        .contains("64 MiB"));
    assert!(import_bytes("bad.txt", &[0xff], &mut notebook, ImportMode::Copy).is_err());
    assert!(import_bytes("app.exe", b"text", &mut notebook, ImportMode::Copy).is_err());
    assert!(import_file(directory.path(), &mut notebook, ImportMode::Copy).is_err());
    assert_eq!(notebook, original);
}

#[test]
fn oversized_html_fails_with_a_visible_limit_error() {
    let html = "x".repeat(MAX_HTML_BYTES + 1);
    let mut notebook = Notebook::default();
    assert!(import_bytes(
        "huge.html",
        html.as_bytes(),
        &mut notebook,
        ImportMode::Copy
    )
    .unwrap_err()
    .contains("8 MiB"));
    assert!(notebook.notes.is_empty());
}

#[test]
fn ambiguous_duplicate_json_keys_fail_without_discarding_notes() {
    let mut notebook = Notebook::default();
    for raw in [
        r#"{"schema_version":1,"notes":[{"important":"content"}],"notes":[],"folders":[],"tags":[]}"#,
        r#"{"notes":[{"id":"first","id":"second"}]}"#,
    ] {
        let error = import_bytes(
            "ambiguous.json",
            raw.as_bytes(),
            &mut notebook,
            ImportMode::Copy,
        )
        .unwrap_err();
        assert!(error.contains("duplicate JSON key"));
        assert!(notebook.notes.is_empty());
    }
}
