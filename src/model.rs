use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
/// Bound record and relationship counts as well as serialized file size.
pub const MAX_RECORDS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notebook {
    pub schema_version: u32,
    pub notes: Vec<Note>,
    pub folders: Vec<Folder>,
    pub tags: Vec<Tag>,
    /// Original browser settings retained for reversible migration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legacy_settings: Vec<serde_json::Value>,
    /// Complete original legacy envelopes keep unknown migration metadata intact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legacy_archives: Vec<serde_json::Value>,
}

impl Default for Notebook {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            notes: Vec::new(),
            folders: Vec::new(),
            tags: Vec::new(),
            legacy_settings: Vec::new(),
            legacy_archives: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    /// Kept as inert text for lossless legacy migration; never rendered as HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_html: Option<String>,
    /// Original browser row/account metadata; inert data, never credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_metadata: Option<serde_json::Value>,
    pub folder_id: Option<String>,
    pub tag_ids: Vec<String>,
    pub is_pinned: bool,
    pub is_deleted: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub version: u64,
}

impl Note {
    pub fn new(title: String, content: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            content,
            original_html: None,
            legacy_metadata: None,
            folder_id: None,
            tag_ids: Vec::new(),
            is_pinned: false,
            is_deleted: false,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
            version: 1,
        }
    }

    pub fn update(&mut self, title: String, content: String) {
        if self.title != title || self.content != content {
            self.title = title;
            self.content = content;
            self.touch();
        }
    }

    pub fn set_deleted(&mut self, deleted: bool) {
        if self.is_deleted != deleted {
            self.is_deleted = deleted;
            self.touch();
            self.deleted_at = deleted.then(|| self.updated_at.clone());
        }
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        if self.is_pinned != pinned {
            self.is_pinned = pinned;
            self.touch();
        }
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now().to_rfc3339();
        self.version = self.version.saturating_add(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

impl Notebook {
    /// Reject incompatible/corrupt snapshots before they can overwrite a good one.
    pub fn validate(&self) -> Result<(), String> {
        let records = self
            .notes
            .len()
            .saturating_add(self.folders.len())
            .saturating_add(self.tags.len())
            .saturating_add(self.legacy_settings.len())
            .saturating_add(self.legacy_archives.len());
        if records > MAX_RECORDS
            || self
                .notes
                .iter()
                .any(|note| note.tag_ids.len() > MAX_RECORDS)
        {
            return Err("笔记、文件夹、标签或保留元数据数量超过 100000 条安全上限。".into());
        }
        for value in self
            .legacy_settings
            .iter()
            .chain(&self.legacy_archives)
            .chain(
                self.notes
                    .iter()
                    .filter_map(|note| note.legacy_metadata.as_ref()),
            )
        {
            validate_metadata(value, 0)?;
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "不支持的数据版本 {}（当前支持 {}），请保留原文件并使用兼容版本打开。",
                self.schema_version, SCHEMA_VERSION
            ));
        }
        let note_ids = unique_ids(self.notes.iter().map(|note| note.id.as_str()), "笔记")?;
        let folder_ids = unique_ids(
            self.folders.iter().map(|folder| folder.id.as_str()),
            "文件夹",
        )?;
        let tag_ids = unique_ids(self.tags.iter().map(|tag| tag.id.as_str()), "标签")?;
        debug_assert_eq!(note_ids.len(), self.notes.len());

        let folders: HashMap<&str, &Folder> = self
            .folders
            .iter()
            .map(|folder| (folder.id.as_str(), folder))
            .collect();
        let mut validated_folders = HashSet::new();
        for folder in &self.folders {
            if validated_folders.contains(folder.id.as_str()) {
                continue;
            }
            let mut ancestors = HashSet::new();
            ancestors.insert(folder.id.as_str());
            let mut parent_id = folder.parent_id.as_deref();
            while let Some(id) = parent_id {
                if validated_folders.contains(id) {
                    break;
                }
                if !ancestors.insert(id) {
                    return Err(format!("文件夹 {} 存在循环引用。", folder.id));
                }
                let parent = folders
                    .get(id)
                    .ok_or_else(|| format!("文件夹 {} 引用了不存在的上级文件夹。", folder.id))?;
                parent_id = parent.parent_id.as_deref();
            }
            validated_folders.extend(ancestors);
        }

        for note in &self.notes {
            if note.version == 0 {
                return Err(format!("笔记 {} 的修订版本无效。", note.id));
            }
            for timestamp in [&note.created_at, &note.updated_at] {
                DateTime::parse_from_rfc3339(timestamp)
                    .map_err(|_| format!("笔记 {} 的时间格式无效。", note.id))?;
            }
            if note.is_deleted != note.deleted_at.is_some() {
                return Err(format!("笔记 {} 的回收站状态不一致。", note.id));
            }
            if let Some(timestamp) = &note.deleted_at {
                DateTime::parse_from_rfc3339(timestamp)
                    .map_err(|_| format!("笔记 {} 的删除时间格式无效。", note.id))?;
            }
            if let Some(folder_id) = note.folder_id.as_deref() {
                if !folder_ids.contains(folder_id) {
                    return Err(format!("笔记 {} 引用了不存在的文件夹。", note.id));
                }
            }
            let mut seen = HashSet::new();
            for tag_id in &note.tag_ids {
                if !tag_ids.contains(tag_id.as_str()) || !seen.insert(tag_id) {
                    return Err(format!("笔记 {} 包含缺失或重复的标签。", note.id));
                }
            }
        }
        Ok(())
    }
}

fn validate_metadata(value: &serde_json::Value, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("保留元数据嵌套超过 64 层安全上限。".into());
    }
    match value {
        serde_json::Value::Array(values) => {
            if values.len() > MAX_RECORDS {
                return Err("保留元数据数组超过 100000 条安全上限。".into());
            }
            for value in values {
                validate_metadata(value, depth + 1)?;
            }
        }
        serde_json::Value::Object(values) => {
            if values.len() > MAX_RECORDS {
                return Err("保留元数据对象超过 100000 字段安全上限。".into());
            }
            for value in values.values() {
                validate_metadata(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &str,
) -> Result<HashSet<&'a str>, String> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() || !seen.insert(id) {
            return Err(format!("{kind}包含空白或重复的 ID。"));
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_notebook_is_valid_and_round_trips() {
        let notebook = Notebook::default();
        notebook.validate().unwrap();
        let parsed: Notebook =
            serde_json::from_str(&serde_json::to_string(&notebook).unwrap()).unwrap();
        assert_eq!(notebook, parsed);
    }

    #[test]
    fn edits_increment_revision_only_when_content_changes() {
        let mut note = Note::new("标题".into(), "内容\n第二行".into());
        let original_time = note.updated_at.clone();
        note.update(note.title.clone(), note.content.clone());
        assert_eq!(note.version, 1);
        assert_eq!(note.updated_at, original_time);
        note.update("新标题".into(), String::new());
        assert_eq!(note.version, 2);
        assert!(note.content.is_empty());
        note.set_pinned(true);
        assert_eq!(note.version, 3);
        note.set_pinned(true);
        assert_eq!(note.version, 3);
    }

    #[test]
    fn soft_delete_and_restore_preserve_content_and_metadata() {
        let mut note = Note::new("Keep".into(), "<script>inert plain text</script>".into());
        note.original_html = Some("<p>original</p>".into());
        note.set_deleted(true);
        assert!(note.is_deleted);
        assert!(note.deleted_at.is_some());
        note.set_deleted(false);
        assert!(!note.is_deleted);
        assert_eq!(note.deleted_at, None);
        assert_eq!(note.version, 3);
        assert_eq!(note.original_html.as_deref(), Some("<p>original</p>"));
        assert_eq!(note.content, "<script>inert plain text</script>");
    }

    #[test]
    fn rejects_unsupported_schema_and_duplicate_note_ids() {
        let mut notebook = Notebook {
            schema_version: 99,
            ..Notebook::default()
        };
        assert!(notebook.validate().is_err());
        notebook.schema_version = SCHEMA_VERSION;
        let note = Note::new("One".into(), String::new());
        notebook.notes = vec![note.clone(), note];
        assert!(notebook.validate().is_err());
    }

    #[test]
    fn rejects_dangling_references_cycles_and_invalid_dates() {
        let mut notebook = Notebook::default();
        let mut note = Note::new("One".into(), String::new());
        note.folder_id = Some("missing".into());
        notebook.notes.push(note);
        assert!(notebook.validate().is_err());
        notebook.notes[0].folder_id = None;
        notebook.notes[0].created_at = "invalid".into();
        assert!(notebook.validate().is_err());
        notebook.notes.clear();
        notebook.folders.push(Folder {
            id: "a".into(),
            name: "A".into(),
            parent_id: Some("b".into()),
            sort_order: 0,
        });
        notebook.folders.push(Folder {
            id: "b".into(),
            name: "B".into(),
            parent_id: Some("a".into()),
            sort_order: 0,
        });
        assert!(notebook.validate().is_err());
    }

    #[test]
    fn rejects_unknown_fields_instead_of_silently_discarding_them() {
        let raw =
            r#"{"schema_version":1,"notes":[],"folders":[],"tags":[],"future_notes":["keep"]}"#;
        assert!(serde_json::from_str::<Notebook>(raw).is_err());
    }
}
