use super::*;
use crate::model::{Folder, Tag};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use serde_json::json;

fn native_note(id: &str) -> Note {
    let mut note = Note::new("笔记".into(), "Hello & 你好".into());
    note.id = id.into();
    note.is_pinned = true;
    note.version = 3;
    note.created_at = "2025-01-01T00:00:00Z".into();
    note.updated_at = "2025-01-02T00:00:00Z".into();
    note
}

fn import_native(
    source: &Notebook,
    notebook: &mut Notebook,
    mode: ImportMode,
) -> Result<ImportReport, String> {
    import_bytes(
        "backup.nebula",
        &nebula_format::encode(source).unwrap(),
        notebook,
        mode,
    )
}

// Authenticate deliberately invalid inner payloads so these tests exercise the
// import validation/rollback path rather than failing the envelope checksum.
fn unchecked_nebula(raw_notebook: &str) -> Vec<u8> {
    let raw = format!(r#"{{"notebook":{raw_notebook},"legacy_source_sha256":null}}"#);
    let nonce = [7_u8; 24];
    let mut header = nebula_format::MAGIC.to_vec();
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&0_u16.to_le_bytes());
    header.extend_from_slice(&nonce);
    header.extend_from_slice(&((raw.len() + nebula_format::TAG_BYTES) as u64).to_le_bytes());
    let cipher = XChaCha20Poly1305::new(b"Nebulabook public format key v1!".into());
    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: raw.as_bytes(),
                aad: &header,
            },
        )
        .unwrap();
    header.extend_from_slice(&ciphertext);
    header
}

fn import_unchecked(
    source: Value,
    notebook: &mut Notebook,
    mode: ImportMode,
) -> Result<ImportReport, String> {
    import_bytes(
        "backup.nebula",
        &unchecked_nebula(&source.to_string()),
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
fn malformed_native_payloads_unknown_fields_and_future_schema_are_nondestructive() {
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
                "bad.nebula",
                &unchecked_nebula(source),
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
    let source = json!({
        "schema_version": 1,
        "notes": [native_note("valid"), {"id": "bad"}],
        "folders": [], "tags": []
    });
    assert!(import_unchecked(source, &mut notebook, ImportMode::Copy).is_err());
    assert_eq!(notebook, original);
}

#[test]
fn default_copy_imports_duplicate_external_ids_without_overwriting() {
    let source = Notebook {
        notes: vec![native_note("external-id")],
        ..Notebook::default()
    };
    let mut notebook = Notebook::default();
    let first = import_native(&source, &mut notebook, ImportMode::default()).unwrap();
    notebook.notes[0].update("Edited locally".into(), "Important new text".into());
    let keep = notebook.notes[0].clone();
    let second = import_native(&source, &mut notebook, ImportMode::default()).unwrap();
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0], keep);
    assert_ne!(notebook.notes[0].id, notebook.notes[1].id);
    assert_ne!(notebook.notes[1].id, "external-id");
    assert_eq!(first.added, 1);
    assert_eq!(second.updated, 0);
}

#[test]
fn duplicate_ids_inside_a_backup_are_rejected_without_partial_changes() {
    let mut notebook = Notebook::default();
    let original = notebook.clone();
    let source = Notebook {
        notes: vec![native_note("same"), native_note("same")],
        ..Notebook::default()
    };
    assert!(import_unchecked(
        serde_json::to_value(source).unwrap(),
        &mut notebook,
        ImportMode::Copy
    )
    .is_err());
    assert_eq!(notebook, original);
}

#[test]
fn explicit_merge_updates_matching_id_and_keeps_unrelated_notes() {
    let mut notebook = Notebook::default();
    let mut source = Notebook {
        notes: vec![native_note("external-id")],
        ..Notebook::default()
    };
    import_native(&source, &mut notebook, ImportMode::Merge).unwrap();
    notebook
        .notes
        .push(Note::new("Unrelated".into(), "Keep".into()));
    let unrelated = notebook.notes[1].clone();
    source.notes[0].title = "Explicit replacement".into();
    let report = import_native(&source, &mut notebook, ImportMode::Merge).unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(report.added, 0);
    assert_eq!(notebook.notes.len(), 2);
    assert_eq!(notebook.notes[0].title, "Explicit replacement");
    assert_eq!(notebook.notes[1], unrelated);
}

fn full_native_notebook() -> Notebook {
    let mut note = native_note("note-original");
    note.folder_id = Some("folder-child".into());
    note.tag_ids = vec!["tag-original".into()];
    note.original_html = Some("<p>Hello &amp; 你好</p>".into());
    // Already-saved metadata is opaque data. It must survive new imports and
    // exports without interpreting it as an old browser format or settings.
    note.legacy_metadata = Some(json!({"retained": "note metadata"}));
    Notebook {
        notes: vec![note],
        folders: vec![
            Folder {
                id: "folder-parent".into(),
                name: "Parent".into(),
                parent_id: None,
                sort_order: 0,
            },
            Folder {
                id: "folder-child".into(),
                name: "Child".into(),
                parent_id: Some("folder-parent".into()),
                sort_order: 1,
            },
        ],
        tags: vec![Tag {
            id: "tag-original".into(),
            name: "Work".into(),
            color: "#112233".into(),
        }],
        legacy_settings: vec![json!({"retained": "settings metadata"})],
        legacy_archives: vec![json!({"retained": "archive metadata"})],
        ..Notebook::default()
    }
}

#[test]
fn copied_native_ids_remap_folder_hierarchy_and_tags_together() {
    let source = full_native_notebook();
    let mut notebook = Notebook::default();
    import_native(&source, &mut notebook, ImportMode::Copy).unwrap();
    notebook.validate().unwrap();
    let note = &notebook.notes[0];
    let child = notebook
        .folders
        .iter()
        .find(|folder| folder.name == "Child")
        .unwrap();
    let parent = notebook
        .folders
        .iter()
        .find(|folder| folder.name == "Parent")
        .unwrap();
    assert_eq!(note.folder_id.as_deref(), Some(child.id.as_str()));
    assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
    assert_ne!(child.id, "folder-child");
    assert_ne!(parent.id, "folder-parent");
    assert_ne!(note.id, "note-original");
    assert_eq!(note.tag_ids, vec![notebook.tags[0].id.clone()]);
    assert_ne!(notebook.tags[0].id, "tag-original");
    assert_eq!(note.original_html, source.notes[0].original_html);
    assert_eq!(note.legacy_metadata, source.notes[0].legacy_metadata);
    assert_eq!(notebook.legacy_settings, source.legacy_settings);
    assert_eq!(notebook.legacy_archives, source.legacy_archives);
}

#[test]
fn invalid_native_folder_and_tag_references_are_rejected_atomically() {
    for bad in ["folder", "tag", "parent", "cycle"] {
        let mut source = full_native_notebook();
        match bad {
            "folder" => source.notes[0].folder_id = Some("missing".into()),
            "tag" => source.notes[0].tag_ids = vec!["missing".into()],
            "parent" => source.folders[1].parent_id = Some("missing".into()),
            "cycle" => source.folders[0].parent_id = Some("folder-child".into()),
            _ => unreachable!(),
        }
        let mut notebook = Notebook {
            notes: vec![native_note("existing")],
            ..Notebook::default()
        };
        let original = notebook.clone();
        assert!(import_unchecked(
            serde_json::to_value(source).unwrap(),
            &mut notebook,
            ImportMode::Copy
        )
        .is_err());
        assert_eq!(notebook, original);
    }
}

#[test]
fn full_native_backup_round_trips_all_metadata_and_trashed_notes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.nebula");
    let mut notebook = full_native_notebook();
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
fn ambiguous_duplicate_native_payload_keys_fail_without_discarding_notes() {
    let mut notebook = Notebook::default();
    for raw in [
        r#"{"schema_version":1,"notes":[{"important":"content"}],"notes":[],"folders":[],"tags":[]}"#,
        r#"{"notes":[{"id":"first","id":"second"}]}"#,
    ] {
        let error = import_bytes(
            "ambiguous.nebula",
            &unchecked_nebula(raw),
            &mut notebook,
            ImportMode::Copy,
        )
        .unwrap_err();
        assert!(error.contains("duplicate JSON key"));
        assert!(notebook.notes.is_empty());
    }
}

#[test]
fn plaintext_json_and_browser_backups_are_not_import_formats() {
    let native = serde_json::to_vec(&full_native_notebook()).unwrap();
    let browser =
        br#"{"format":"nebula-legacy-indexeddb","schemaVersion":1,"stores":{"notes":[]}}"#;
    let mut notebook = Notebook {
        notes: vec![native_note("existing")],
        ..Notebook::default()
    };
    let original = notebook.clone();
    for source in [native.as_slice(), browser.as_slice()] {
        let error =
            import_bytes("backup.json", source, &mut notebook, ImportMode::Copy).unwrap_err();
        assert!(error.contains("Unsupported import format"));
        // Renaming a plaintext backup cannot make it a valid native envelope.
        assert!(import_bytes("backup.nebula", source, &mut notebook, ImportMode::Copy).is_err());
        assert_eq!(notebook, original);
    }
}

#[test]
fn native_import_corruption_and_unsupported_versions_leave_data_unchanged() {
    let bytes = nebula_format::encode(&full_native_notebook()).unwrap();
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    let mut future = bytes.clone();
    future[8] = 99;
    let mut notebook = Notebook {
        notes: vec![native_note("existing")],
        ..Notebook::default()
    };
    let original = notebook.clone();
    for source in [
        corrupt.as_slice(),
        future.as_slice(),
        &bytes[..bytes.len() - 1],
    ] {
        assert!(import_bytes("backup.nebula", source, &mut notebook, ImportMode::Merge).is_err());
        assert_eq!(notebook, original);
    }
}

#[test]
fn backup_export_requires_native_extension_before_creating_a_file() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["backup.json", "backup.txt", "backup"] {
        let path = directory.path().join(name);
        assert!(export_backup(&path, &Notebook::default()).is_err());
        assert!(!path.exists());
    }
    let path = directory.path().join("backup.NEBULA");
    export_backup(&path, &full_native_notebook()).unwrap();
    let mut restored = Notebook::default();
    import_file(&path, &mut restored, ImportMode::Merge).unwrap();
    assert_eq!(restored, full_native_notebook());
}

#[test]
fn oversized_native_payload_collections_are_rejected_atomically() {
    let mut notebook = Notebook {
        notes: vec![native_note("existing")],
        ..Notebook::default()
    };
    let original = notebook.clone();
    let array = vec![Value::Null; crate::model::MAX_RECORDS + 1];
    let object: serde_json::Map<_, _> = (0..=crate::model::MAX_RECORDS)
        .map(|index| (index.to_string(), Value::Null))
        .collect();
    for metadata in [Value::Array(array), Value::Object(object)] {
        let mut source = full_native_notebook();
        source.notes[0].legacy_metadata = Some(metadata);
        let error = import_unchecked(
            serde_json::to_value(source).unwrap(),
            &mut notebook,
            ImportMode::Copy,
        )
        .unwrap_err();
        assert!(error.contains("100000"));
        assert_eq!(notebook, original);
    }
}

#[test]
fn exceeding_combined_record_limit_rolls_back_a_valid_native_import() {
    let mut notebook = Notebook {
        legacy_settings: vec![Value::Null; crate::model::MAX_RECORDS],
        ..Notebook::default()
    };
    notebook.validate().unwrap();
    let original = notebook.clone();
    let source = Notebook {
        notes: vec![native_note("incoming")],
        ..Notebook::default()
    };
    let error = import_native(&source, &mut notebook, ImportMode::Copy).unwrap_err();
    assert!(error.contains("100000"));
    assert_eq!(notebook, original);
}
