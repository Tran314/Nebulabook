//! Explicit, local-only migration and exports. Parsing never changes the current
//! notebook; the fully validated candidate is installed only after every check.
use crate::model::{Folder, Note, Notebook, Tag};
use crate::storage::MAX_NOTEBOOK_BYTES;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;

pub const MAX_IMPORT_BYTES: u64 = MAX_NOTEBOOK_BYTES;
pub const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImportMode {
    /// Give every imported record a fresh ID, preserving all existing records.
    #[default]
    Copy,
    /// Explicitly replace records with matching IDs; retain unrelated records.
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteFormat {
    Text,
    Markdown,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    pub notes_imported: usize,
    pub warnings: Vec<String>,
}

pub fn import_file(
    path: &Path,
    notebook: &mut Notebook,
    mode: ImportMode,
) -> Result<ImportReport, String> {
    let bytes = read_bounded(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Imported note");
    import_bytes(name, &bytes, notebook, mode)
}

/// Also useful for tests and future native drag-and-drop; the filename determines
/// the explicit format. Unsupported extensions never fall back to JSON guessing.
pub fn import_bytes(
    filename: &str,
    bytes: &[u8],
    notebook: &mut Notebook,
    mode: ImportMode,
) -> Result<ImportReport, String> {
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Import exceeds the 64 MiB file limit.".into());
    }
    let source =
        std::str::from_utf8(bytes).map_err(|_| "Import must be valid UTF-8 text.".to_string())?;
    let text = source.strip_prefix('\u{feff}').unwrap_or(source);
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut warnings = Vec::new();
    let mut imported = match extension.as_str() {
        "json" => parse_json(text, &mut warnings)?,
        "txt" | "md" | "markdown" | "html" | "htm" => {
            let title = Path::new(filename)
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Imported note");
            let html = matches!(extension.as_str(), "html" | "htm");
            let content = if html {
                html_to_text(text)?
            } else {
                text.to_string()
            };
            let mut note = Note::new(title.to_string(), content);
            if html {
                note.original_html = Some(source.to_string());
                warnings.push("HTML was converted to editable text without executing scripts or loading remote resources. The original HTML is retained in JSON backups.".into());
            }
            let mut imported = Notebook::default();
            imported.notes.push(note);
            imported
        }
        _ => return Err("Unsupported import format. Choose TXT, Markdown, HTML, or JSON.".into()),
    };

    imported
        .validate()
        .map_err(|error| format!("Import validation failed: {error}"))?;
    notebook
        .validate()
        .map_err(|error| format!("Current notebook is invalid: {error}"))?;
    let mut candidate = notebook.clone();
    let mut report = ImportReport {
        notes_imported: imported.notes.len(),
        warnings,
        ..ImportReport::default()
    };
    match mode {
        ImportMode::Copy => {
            remap_ids(&mut imported);
            report.added = imported.notes.len();
            candidate.notes.extend(imported.notes);
            candidate.folders.extend(imported.folders);
            candidate.tags.extend(imported.tags);
        }
        ImportMode::Merge => {
            for folder in imported.folders {
                if let Some(existing) = candidate
                    .folders
                    .iter_mut()
                    .find(|item| item.id == folder.id)
                {
                    *existing = folder;
                } else {
                    candidate.folders.push(folder);
                }
            }
            for tag in imported.tags {
                if let Some(existing) = candidate.tags.iter_mut().find(|item| item.id == tag.id) {
                    *existing = tag;
                } else {
                    candidate.tags.push(tag);
                }
            }
            for note in imported.notes {
                if let Some(existing) = candidate.notes.iter_mut().find(|item| item.id == note.id) {
                    *existing = note;
                    report.updated += 1;
                } else {
                    candidate.notes.push(note);
                    report.added += 1;
                }
            }
        }
    }
    candidate.legacy_settings.extend(imported.legacy_settings);
    candidate.legacy_archives.extend(imported.legacy_archives);
    // This also catches interactions between an imported graph and existing data.
    candidate
        .validate()
        .map_err(|error| format!("Import would create an invalid notebook: {error}"))?;
    let serialized = serde_json::to_vec_pretty(&candidate)
        .map_err(|error| format!("Cannot encode imported notebook: {error}"))?;
    if serialized.len() as u64 > MAX_IMPORT_BYTES {
        return Err(
            "Import would exceed the notebook's 64 MiB storage limit. Nothing was imported.".into(),
        );
    }
    *notebook = candidate;
    Ok(report)
}

pub fn export_backup(path: &Path, notebook: &Notebook) -> Result<(), String> {
    notebook
        .validate()
        .map_err(|error| format!("Cannot export invalid notebook: {error}"))?;
    let bytes = serde_json::to_vec_pretty(notebook)
        .map_err(|error| format!("Cannot encode backup: {error}"))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Backup exceeds the 64 MiB storage limit.".into());
    }
    write_new(path, &bytes)
}

/// Markdown is exported as the editor's Markdown source, never rendered HTML.
/// A full JSON backup is required to retain IDs, timestamps, tags and raw HTML.
pub fn export_note(path: &Path, note: &Note, format: NoteFormat) -> Result<(), String> {
    let text = match format {
        NoteFormat::Text | NoteFormat::Markdown => &note.content,
    };
    write_new(path, text.as_bytes())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    // Reject devices/FIFOs before open, which could otherwise block the UI when
    // a path is entered manually. Recheck the opened file below as well.
    if !fs::metadata(path)
        .map_err(|error| format!("Cannot inspect import: {error}"))?
        .is_file()
    {
        return Err("Choose a regular file to import.".into());
    }
    let file = File::open(path).map_err(|error| format!("Cannot open import: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot inspect import: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose a regular file to import.".into());
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err("Import exceeds the 64 MiB file limit.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read import: {error}"))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Import exceeds the 64 MiB file limit.".into());
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            "That file already exists. Choose a new filename; exports never overwrite an existing file.".to_string()
        } else {
            format!("Cannot create export: {error}")
        }
    })?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("Cannot complete export: {error}"));
    }
    Ok(())
}

fn html_to_text(html: &str) -> Result<String, String> {
    if html.len() > MAX_HTML_BYTES {
        return Err("HTML content exceeds the 8 MiB conversion limit.".into());
    }
    // html2text uses an HTML parser, not a browser: no JavaScript, iframe,
    // network access, event handlers, or embedded content is executed.
    html2text::from_read(html.as_bytes(), 120)
        .map_err(|error| format!("Cannot convert HTML: {error}"))
}

fn remap_ids(notebook: &mut Notebook) {
    let folders: HashMap<String, String> = notebook
        .folders
        .iter()
        .map(|folder| (folder.id.clone(), Uuid::new_v4().to_string()))
        .collect();
    let tags: HashMap<String, String> = notebook
        .tags
        .iter()
        .map(|tag| (tag.id.clone(), Uuid::new_v4().to_string()))
        .collect();
    for folder in &mut notebook.folders {
        folder.id = folders[&folder.id].clone();
        folder.parent_id = folder.parent_id.as_ref().map(|id| folders[id].clone());
    }
    for tag in &mut notebook.tags {
        tag.id = tags[&tag.id].clone();
    }
    for note in &mut notebook.notes {
        note.id = Uuid::new_v4().to_string();
        note.folder_id = note.folder_id.as_ref().map(|id| folders[id].clone());
        note.tag_ids = note.tag_ids.iter().map(|id| tags[id].clone()).collect();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyNote {
    id: String,
    user_id: String,
    folder_id: Option<String>,
    title: String,
    content: Option<String>,
    is_pinned: bool,
    is_deleted: bool,
    deleted_at: Option<String>,
    version: u64,
    created_at: String,
    updated_at: String,
    tags: Vec<String>,
    synced_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyFolder {
    id: String,
    user_id: String,
    name: String,
    parent_id: Option<String>,
    sort_order: i32,
    created_at: String,
    updated_at: String,
    synced_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyTag {
    id: String,
    user_id: String,
    name: String,
    color: String,
    created_at: String,
    synced_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStores {
    notes: Vec<Value>,
    #[serde(default)]
    folders: Vec<Value>,
    #[serde(default)]
    tags: Vec<Value>,
    #[serde(default)]
    settings: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyExport {
    format: String,
    schema_version: u32,
    exported_at: String,
    database: LegacyDatabase,
    stores: LegacyStores,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDatabase {
    name: String,
    version: u64,
}

/// Value's default deserializer silently keeps the last duplicate object key.
/// Reject ambiguous backups instead of importing a potentially truncated record.
struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(value)))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJson(Value::Number(number)))
                    .ok_or_else(|| E::custom("JSON number is not finite"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(value.to_string())))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(value)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key: {key}"
                        )));
                    }
                    values.insert(key, map.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn parse_json(text: &str, warnings: &mut Vec<String>) -> Result<Notebook, String> {
    let value = serde_json::from_str::<UniqueJson>(text)
        .map_err(|error| format!("Invalid JSON: {error}"))?
        .0;
    if value.get("schema_version").is_some() {
        return serde_json::from_value(value)
            .map_err(|error| format!("Invalid native backup schema: {error}"));
    }
    let archive = value.clone();
    let stores = if value.is_array() {
        LegacyStores {
            notes: serde_json::from_value(value).map_err(|error| error.to_string())?,
            folders: vec![],
            tags: vec![],
            settings: vec![],
        }
    } else if value.get("format").is_some() {
        let export: LegacyExport = serde_json::from_value(value)
            .map_err(|error| format!("Invalid legacy export schema: {error}"))?;
        if export.format != "nebula-legacy-indexeddb"
            || export.schema_version != 1
            || export.database.name != "NebulaLocalDB"
            || !matches!(export.database.version, 1 | 10)
        {
            // Dexie schema v1 uses native IndexedDB version 10.
            return Err("Unsupported legacy export format or database version.".into());
        }
        validate_timestamp(&export.exported_at)?;
        export.stores
    } else {
        serde_json::from_value(value)
            .map_err(|error| format!("Unrecognized JSON backup schema: {error}"))?
    };
    let mut notebook = parse_legacy_stores(stores, warnings)?;
    notebook.legacy_archives.push(archive);
    Ok(notebook)
}

fn validate_timestamp(value: &str) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| "Legacy import contains an invalid RFC3339 timestamp.".into())
}

fn validate_legacy_metadata(
    user_id: &str,
    created_at: &str,
    synced_at: Option<&str>,
) -> Result<(), String> {
    if user_id.trim().is_empty() {
        return Err("Legacy records must have a nonempty userId.".into());
    }
    validate_timestamp(created_at)?;
    if let Some(timestamp) = synced_at {
        validate_timestamp(timestamp)?;
    }
    Ok(())
}

fn parse_legacy_stores(
    stores: LegacyStores,
    warnings: &mut Vec<String>,
) -> Result<Notebook, String> {
    let mut notebook = Notebook::default();
    let mut folder_owners = HashMap::new();
    let mut tag_owners = HashMap::new();
    for raw in stores.folders {
        let folder: LegacyFolder = serde_json::from_value(raw)
            .map_err(|error| format!("Invalid legacy folder: {error}"))?;
        validate_legacy_metadata(
            &folder.user_id,
            &folder.created_at,
            folder.synced_at.as_deref(),
        )?;
        validate_timestamp(&folder.updated_at)?;
        folder_owners.insert(folder.id.clone(), folder.user_id);
        notebook.folders.push(Folder {
            id: folder.id,
            name: folder.name,
            parent_id: folder.parent_id,
            sort_order: folder.sort_order,
        });
    }
    for folder in &notebook.folders {
        if let Some(parent_id) = &folder.parent_id {
            if folder_owners.get(&folder.id) != folder_owners.get(parent_id) {
                return Err("Legacy folder has an unavailable or cross-user parent. Import a complete export.".into());
            }
        }
    }
    for raw in stores.tags {
        let tag: LegacyTag =
            serde_json::from_value(raw).map_err(|error| format!("Invalid legacy tag: {error}"))?;
        validate_legacy_metadata(&tag.user_id, &tag.created_at, tag.synced_at.as_deref())?;
        tag_owners.insert(tag.id.clone(), tag.user_id);
        notebook.tags.push(Tag {
            id: tag.id,
            name: tag.name,
            color: tag.color,
        });
    }
    for raw in stores.notes {
        let old: LegacyNote = serde_json::from_value(raw.clone())
            .map_err(|error| format!("Invalid legacy note: {error}"))?;
        validate_legacy_metadata(&old.user_id, &old.created_at, old.synced_at.as_deref())?;
        if let Some(folder_id) = &old.folder_id {
            if folder_owners.get(folder_id) != Some(&old.user_id) {
                return Err("Legacy note has an unavailable or cross-user folder. Import a complete export.".into());
            }
        }
        for tag_id in &old.tags {
            if tag_owners.get(tag_id) != Some(&old.user_id) {
                return Err(
                    "Legacy note has an unavailable or cross-user tag. Import a complete export."
                        .into(),
                );
            }
        }
        let html = old.content.unwrap_or_default();
        let content = html_to_text(&html)?;
        let mut note = Note::new(old.title, content);
        note.id = old.id;
        note.original_html = Some(html);
        note.legacy_metadata = Some(raw);
        note.folder_id = old.folder_id;
        note.tag_ids = old.tags;
        note.is_pinned = old.is_pinned;
        note.is_deleted = old.is_deleted;
        note.deleted_at = old.deleted_at;
        note.created_at = old.created_at;
        note.updated_at = old.updated_at;
        note.version = old.version;
        notebook.notes.push(note);
    }
    warnings.push("Legacy HTML was converted to editable text. Original note HTML and the complete legacy JSON data are archived in native JSON backups, including folder/tag timestamps and browser settings.".into());
    if !stores.settings.is_empty() {
        warnings.push("Browser-specific settings are archived in JSON backups but are not applied to the native app.".into());
    }
    notebook.legacy_settings = stores.settings;
    Ok(notebook)
}

#[cfg(test)]
mod tests;
