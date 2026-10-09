#![cfg(target_os = "linux")]

use nebulabook::import_export::{export_backup, export_note, NoteFormat};
use nebulabook::model::Note;
use nebulabook::storage::Storage;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[test]
fn default_storage_follows_absolute_xdg_data_home_and_home_fallback() {
    // Give each probe its own process environment instead of racing other tests
    // by mutating HOME/XDG_DATA_HOME in this multithreaded test process.
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let xdg = directory.path().join("custom 数据");
    for value in [
        Some(xdg.as_os_str()),
        Some(std::ffi::OsStr::new("relative-data")),
        Some(std::ffi::OsStr::new("")),
        None,
    ] {
        let expected = if value == Some(xdg.as_os_str()) {
            xdg.clone()
        } else {
            home.join(".local/share")
        }
        .join("nebulanotepad/notebook.nebula");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", "xdg_data_path_child_probe", "--nocapture"])
            .env("HOME", &home)
            .env("NEBULABOOK_TEST_EXPECTED_DATA", &expected)
            .env_remove("XDG_DATA_HOME");
        if let Some(value) = value {
            child.env("XDG_DATA_HOME", value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn xdg_data_path_child_probe() {
    let Some(expected) = std::env::var_os("NEBULABOOK_TEST_EXPECTED_DATA") else {
        return;
    };
    let (storage, notebook) = Storage::open_default().unwrap();
    assert_eq!(storage.path(), Path::new(&expected));
    if std::env::var_os("NEBULABOOK_TEST_EXISTING_NOTE").is_some() {
        assert_eq!(notebook.notes.len(), 1);
        assert_eq!(notebook.notes[0].title, "旧 Nebula 的笔记");
        assert_eq!(notebook.notes[0].content, "改名后继续读取，不复制、不重置");
    }
}

#[test]
fn new_data_directories_and_snapshot_lock_backup_exports_are_private() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let path = root.join("new-parent/notebook/notebook.nebula");
    let (mut storage, mut notebook) = Storage::open(path.clone()).unwrap();
    notebook
        .notes
        .push(Note::new("私密".into(), "中文正文".into()));
    storage.save(&notebook).unwrap();
    notebook.notes[0].update("更新".into(), "修改后的正文".into());
    storage.save(&notebook).unwrap();
    for directory in [
        root.join("new-parent"),
        path.parent().unwrap().to_path_buf(),
    ] {
        assert_eq!(
            std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    let backup = root.join("export.nebula");
    let note = root.join("export.txt");
    export_backup(&backup, &notebook).unwrap();
    export_note(&note, &notebook.notes[0], NoteFormat::Text).unwrap();
    for file in [
        &path,
        &path.with_file_name("notebook.json.lock"),
        &storage.backup_path(),
        &backup,
        &note,
    ] {
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600,
            "{}",
            file.display()
        );
    }
}

#[test]
fn existing_directory_permissions_are_not_changed() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o750)).unwrap();
    let (_storage, _) = Storage::open(directory.path().join("notebook.json")).unwrap();
    assert_eq!(
        std::fs::metadata(directory.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
}

#[test]
fn renamed_app_migrates_in_existing_directory_without_overwriting_plaintext() {
    let directory = tempfile::tempdir().unwrap();
    let xdg = directory.path().join("data");
    let legacy_path = xdg.join("nebulanotepad/notebook.json");
    let mut notebook = nebulabook::model::Notebook::default();
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    notebook.notes.push(Note::new(
        "旧 Nebula 的笔记".into(),
        "改名后继续读取，不复制、不重置".into(),
    ));
    std::fs::write(&legacy_path, serde_json::to_vec_pretty(&notebook).unwrap()).unwrap();
    let before = std::fs::read(&legacy_path).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "xdg_data_path_child_probe", "--nocapture"])
        .env("XDG_DATA_HOME", &xdg)
        .env(
            "NEBULABOOK_TEST_EXPECTED_DATA",
            legacy_path.with_extension("nebula"),
        )
        .env("NEBULABOOK_TEST_EXISTING_NOTE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&legacy_path).unwrap(), before);
    assert!(legacy_path.with_extension("nebula").exists());
    assert_eq!(
        nebulabook::nebula_format::decode(
            &std::fs::read(legacy_path.with_extension("migration.nebula")).unwrap()
        )
        .unwrap(),
        notebook
    );
    assert!(!xdg.join("nebulabook").exists());
}
