use crate::model::Notebook;
use crate::nebula_format;
use directories::ProjectDirs;
use fs2::FileExt;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(test)]
use uuid::Uuid;

#[derive(Debug)]
pub enum StorageError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidData {
        path: PathBuf,
        message: String,
    },
    Locked(PathBuf),
    RecoveryAvailable {
        path: PathBuf,
        backup: PathBuf,
    },
    Conflict(PathBuf),
    TooLarge(PathBuf),
    NoDataDirectory,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, path, source } => write!(f, "{operation}失败（{}）：{source}", path.display()),
            Self::InvalidData { path, message } => write!(f, "无法读取或保存 {}：{message} 原文件未被覆盖。若需恢复，请先关闭程序，保留或重命名损坏的主文件，再将已确认完好的备份 {} 复制为 {}，然后重新打开程序或重试。", path.display(), backup_path(path).display(), path.display()),
            Self::RecoveryAvailable { path, backup } => write!(f, "主数据文件 {} 不存在，但发现备份 {}。未创建空白数据；请先复制备份为主数据文件，再重新打开程序。", path.display(), backup.display()),
            Self::Locked(path) => write!(f, "数据文件已被另一实例使用：{}。请关闭另一个记事本窗口后重试。", path.display()),
            Self::Conflict(path) => write!(f, "数据文件已被外部修改：{}。为避免覆盖，保存已停止。请先导出当前笔记，再重新打开程序。", path.display()),
            Self::TooLarge(path) => write!(f, "数据文件超过 64 MiB 安全上限：{}。原文件未被覆盖。", path.display()),
            Self::NoDataDirectory => write!(f, "无法确定本机用户数据目录。"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One local snapshot, one previous-snapshot backup, and an OS-held instance lock.
/// A failed open never creates an empty replacement for an existing data file.
#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
    _lock: File,
    current_bytes: Option<Vec<u8>>,
    current_notebook_digest: Option<String>,
}

impl Storage {
    pub fn open_default() -> Result<(Self, Notebook), StorageError> {
        // Stable application identity: branding/bin/repository changes must never
        // move or hide an existing notebook, backup or instance lock.
        let directories = ProjectDirs::from("com", "Nebula", "Nebula Notepad")
            .ok_or(StorageError::NoDataDirectory)?;
        Self::open(directories.data_local_dir().join("notebook.nebula"))
    }

    pub fn open(path: PathBuf) -> Result<(Self, Notebook), StorageError> {
        // Resolve relative filenames before selecting a same-directory temp path.
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|error| io_error("读取工作目录", &path, error))?
                .join(path)
        };
        if path
            .extension()
            .is_none_or(|extension| extension != "nebula")
        {
            return Err(StorageError::InvalidData {
                path,
                message: "本机数据文件必须使用小写 .nebula 后缀；其他支持的文本文件请显式导入。"
                    .into(),
            });
        }
        let parent = path.parent().ok_or_else(|| StorageError::InvalidData {
            path: path.clone(),
            message: "数据文件路径无效。".into(),
        })?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            // Restrict newly created app directories without changing permissions
            // on an existing user-selected directory or its shared ancestors.
            builder.mode(0o700);
        }
        builder
            .create(parent)
            .map_err(|error| io_error("创建数据目录", parent, error))?;
        // Keep the established lock filename so every current-format writer
        // coordinates on the same operating-system lock. No JSON is read.
        let lock_path = path.with_extension("json.lock");
        let lock = private_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io_error("打开数据锁", &lock_path, error))?;
        lock.try_lock_exclusive().map_err(|error| {
            if is_lock_contention(&error) {
                StorageError::Locked(path.clone())
            } else {
                io_error("锁定数据文件", &lock_path, error)
            }
        })?;
        let current_bytes = read_optional(&path)?;
        let notebook = match &current_bytes {
            Some(bytes) => {
                nebula_format::decode(bytes).map_err(|message| StorageError::InvalidData {
                    path: path.clone(),
                    message,
                })?
            }
            None => {
                // Existing native recovery data must never be hidden by a new
                // empty notebook, including snapshots made by earlier v1 writers.
                for backup in [backup_path(&path), path.with_extension("migration.nebula")] {
                    if backup
                        .try_exists()
                        .map_err(|error| io_error("检查恢复备份", &backup, error))?
                    {
                        return Err(StorageError::RecoveryAvailable { path, backup });
                    }
                }
                Notebook::default()
            }
        };
        let current_notebook_digest = if current_bytes.is_some() {
            Some(notebook_digest(&notebook)?)
        } else {
            None
        };
        Ok((
            Self {
                path,
                _lock: lock,
                current_bytes,
                current_notebook_digest,
            },
            notebook,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn backup_path(&self) -> PathBuf {
        backup_path(&self.path)
    }

    pub fn save(&mut self, notebook: &Notebook) -> Result<(), StorageError> {
        notebook
            .validate()
            .map_err(|message| StorageError::InvalidData {
                path: self.path.clone(),
                message,
            })?;
        let candidate_digest = notebook_digest(notebook)?;

        // Also refuse overwrite if an editor/import tool changed the file despite
        // our cooperative process lock. The user can export their unsaved work.
        if read_optional(&self.path)? != self.current_bytes {
            return Err(StorageError::Conflict(self.path.clone()));
        }
        if self.current_notebook_digest.as_ref() == Some(&candidate_digest) {
            return Ok(());
        }
        let bytes =
            nebula_format::encode(notebook).map_err(|message| StorageError::InvalidData {
                path: self.path.clone(),
                message,
            })?;
        if let Some(previous) = &self.current_bytes {
            atomic_write(&self.backup_path(), previous)?;
        }
        // Recheck after backup I/O as well, before replacing the primary.
        if read_optional(&self.path)? != self.current_bytes {
            return Err(StorageError::Conflict(self.path.clone()));
        }
        if self.current_bytes.is_none() {
            atomic_write_new(&self.path, &bytes)?;
        } else {
            atomic_write(&self.path, &bytes)?;
        }
        self.current_bytes = Some(bytes);
        self.current_notebook_digest = Some(candidate_digest);
        Ok(())
    }
}

fn is_lock_contention(error: &io::Error) -> bool {
    // Windows ERROR_LOCK_VIOLATION is not classified as WouldBlock by std::io.
    // fs2 exposes the platform's exact contention code; missing codes must not
    // compare equal and accidentally classify unrelated I/O failures as locks.
    error.kind() == io::ErrorKind::WouldBlock
        || matches!(
            (error.raw_os_error(), fs2::lock_contended_error().raw_os_error()),
            (Some(actual), Some(expected)) if actual == expected
        )
}

fn notebook_digest(notebook: &Notebook) -> Result<String, StorageError> {
    let bytes = serde_json::to_vec(notebook).map_err(|error| StorageError::InvalidData {
        path: PathBuf::from("notebook.nebula"),
        message: error.to_string(),
    })?;
    Ok(nebula_format::digest(&bytes))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("backup.nebula")
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, StorageError> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(StorageError::InvalidData {
                path: path.to_path_buf(),
                message: "数据路径不是普通文件。".into(),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("检查数据文件", path, error)),
        _ => {}
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("读取数据文件", path, error)),
    };
    if file
        .metadata()
        .map_err(|error| io_error("读取数据文件信息", path, error))?
        .len()
        > nebula_format::MAX_FILE_BYTES
    {
        return Err(StorageError::TooLarge(path.to_path_buf()));
    }
    let mut bytes = Vec::new();
    file.take(nebula_format::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("读取数据文件", path, error))?;
    if bytes.len() as u64 > nebula_format::MAX_FILE_BYTES {
        return Err(StorageError::TooLarge(path.to_path_buf()));
    }
    Ok(Some(bytes))
}

fn private_options() -> OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.mode(0o600);
        options
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new()
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> StorageError {
    StorageError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

/// Write and fsync a private temp file, then atomically replace the destination.
/// The destination is never removed first (including on Windows).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    atomic_commit(path, bytes, false)
}

fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    atomic_commit(path, bytes, true)
}

fn atomic_commit(path: &Path, bytes: &[u8], create_new: bool) -> Result<(), StorageError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| {
        io_error(
            "获取临时文件安全随机数",
            path,
            io::Error::other(error.to_string()),
        )
    })?;
    let unique: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = sibling_with_suffix(path, &format!(".{unique}.tmp"));
    let result = (|| {
        let mut file = private_options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("创建临时数据文件", &temporary, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("写入临时数据文件", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error("同步临时数据文件", &temporary, error))?;
        drop(file);
        if create_new {
            // Linking a synced temporary file publishes it atomically without
            // overwriting a file that appeared after our conflict check.
            fs::hard_link(&temporary, path)
                .map_err(|error| io_error("原子创建数据文件", path, error))?;
            let _ = fs::remove_file(&temporary);
        } else {
            fs::rename(&temporary, path)
                .map_err(|error| io_error("原子替换数据文件", path, error))?;
        }
        // Directory syncing is not supported on every platform/filesystem. The
        // committed file itself was synced above; never report a failed save
        // after a successful atomic replacement solely for directory fsync.
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("nebulabook-storage-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("notebook.nebula")
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creates_and_reopens_a_unicode_notebook() {
        let directory = TestDirectory::new();
        let (mut storage, mut notebook) = Storage::open(directory.file()).unwrap();
        assert!(!directory.file().exists());
        notebook
            .notes
            .push(Note::new("工作记录".into(), "第一行\n第二行 📝".into()));
        storage.save(&notebook).unwrap();
        drop(storage);
        let (_, loaded) = Storage::open(directory.file()).unwrap();
        assert_eq!(loaded, notebook);
    }

    #[test]
    fn keeps_previous_snapshot_and_leaves_no_temporary_files() {
        let directory = TestDirectory::new();
        let (mut storage, mut notebook) = Storage::open(directory.file()).unwrap();
        notebook
            .notes
            .push(Note::new("First".into(), "Keep original".into()));
        storage.save(&notebook).unwrap();
        let previous = fs::read(directory.file()).unwrap();
        notebook.notes[0].update("Second".into(), "Latest".into());
        storage.save(&notebook).unwrap();
        assert_eq!(fs::read(storage.backup_path()).unwrap(), previous);
        assert_eq!(
            nebula_format::decode(&fs::read(directory.file()).unwrap()).unwrap(),
            notebook
        );
        assert!(fs::read_dir(&directory.0).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn malformed_or_unsupported_snapshots_are_never_reset() {
        let directory = TestDirectory::new();
        for invalid in [
            b"broken JSON".as_slice(),
            br#"{"schema_version":99,"notes":[],"folders":[],"tags":[]}"#,
        ] {
            fs::write(directory.file(), invalid).unwrap();
            assert!(matches!(
                Storage::open(directory.file()),
                Err(StorageError::InvalidData { .. })
            ));
            assert_eq!(fs::read(directory.file()).unwrap(), invalid);
        }
    }

    #[test]
    fn a_second_instance_cannot_overwrite_the_first() {
        let directory = TestDirectory::new();
        let (storage, _) = Storage::open(directory.file()).unwrap();
        assert!(matches!(
            Storage::open(directory.file()),
            Err(StorageError::Locked(_))
        ));
        drop(storage);
        assert!(Storage::open(directory.file()).is_ok());
    }

    #[test]
    fn external_edits_are_detected_without_overwrite() {
        let directory = TestDirectory::new();
        let (mut storage, notebook) = Storage::open(directory.file()).unwrap();
        storage.save(&notebook).unwrap();
        let external = b"External replacement that must remain intact";
        fs::write(directory.file(), external).unwrap();
        assert!(matches!(
            storage.save(&notebook),
            Err(StorageError::Conflict(_))
        ));
        assert_eq!(fs::read(directory.file()).unwrap(), external);
    }

    #[test]
    fn invalid_candidate_does_not_change_data_or_backup() {
        let directory = TestDirectory::new();
        let (mut storage, mut notebook) = Storage::open(directory.file()).unwrap();
        storage.save(&notebook).unwrap();
        let previous = fs::read(directory.file()).unwrap();
        notebook.schema_version = 99;
        assert!(storage.save(&notebook).is_err());
        assert_eq!(fs::read(directory.file()).unwrap(), previous);
        assert!(!storage.backup_path().exists());
    }

    #[test]
    fn blocked_backup_prevents_overwriting_the_main_snapshot() {
        let directory = TestDirectory::new();
        let (mut storage, mut notebook) = Storage::open(directory.file()).unwrap();
        storage.save(&notebook).unwrap();
        let previous = fs::read(directory.file()).unwrap();
        fs::create_dir(storage.backup_path()).unwrap();
        notebook
            .notes
            .push(Note::new("Unsaved".into(), "Still in memory".into()));
        assert!(storage.save(&notebook).is_err());
        assert_eq!(fs::read(directory.file()).unwrap(), previous);
    }

    #[test]
    fn missing_primary_with_backup_never_starts_an_empty_notebook() {
        let directory = TestDirectory::new();
        let backup = backup_path(&directory.file());
        let notebook = Notebook::default();
        let bytes = nebula_format::encode(&notebook).unwrap();
        fs::write(&backup, &bytes).unwrap();
        assert!(matches!(
            Storage::open(directory.file()),
            Err(StorageError::RecoveryAvailable { .. })
        ));
        assert!(!directory.file().exists());
        assert_eq!(fs::read(backup).unwrap(), bytes);
    }

    #[test]
    fn oversized_snapshot_is_rejected_without_overwriting_it() {
        let directory = TestDirectory::new();
        let file = File::create(directory.file()).unwrap();
        file.set_len(nebula_format::MAX_FILE_BYTES + 1).unwrap();
        drop(file);
        assert!(matches!(
            Storage::open(directory.file()),
            Err(StorageError::TooLarge(_))
        ));
        assert_eq!(
            fs::metadata(directory.file()).unwrap().len(),
            nebula_format::MAX_FILE_BYTES + 1
        );
    }

    #[test]
    fn classifies_platform_lock_contention_and_preserves_other_io_errors() {
        let code = fs2::lock_contended_error()
            .raw_os_error()
            .expect("fs2 contention error must have a platform error code");
        assert!(is_lock_contention(&io::Error::from_raw_os_error(code)));
        assert!(is_lock_contention(&io::Error::from(
            io::ErrorKind::WouldBlock
        )));
        assert!(!is_lock_contention(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(!is_lock_contention(&io::Error::other("different failure")));
    }

    #[test]
    fn corrupt_main_recovery_instructions_do_not_require_opening_the_notebook() {
        let path = PathBuf::from("notebook.nebula");
        let message = StorageError::InvalidData {
            path,
            message: "损坏的 .nebula".into(),
        }
        .to_string();
        assert!(message.contains("先关闭程序"));
        assert!(message.contains("保留或重命名损坏的主文件"));
        assert!(message.contains("notebook.backup.nebula 复制为 notebook.nebula"));
        assert!(message.contains("重新打开程序或重试"));
        assert!(!message.contains("导入"));
    }
}
