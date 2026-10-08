use crate::fonts::install_system_font;
use crate::import_export::{export_backup, export_note, import_file, ImportMode, NoteFormat};
use crate::model::{Note, Notebook};
use crate::storage::Storage;
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(800);

enum UiAction {
    NewNote,
    Save,
    Select(String),
    ShowTrash(bool),
    SetDeleted(bool),
    TogglePin,
    Import,
    ExportBackup,
    ExportNote(bool),
    Reopen,
    #[cfg(target_os = "linux")]
    ManualPath(PathAction),
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum PathAction {
    Import,
    Backup,
    Note(bool),
}

#[cfg(target_os = "linux")]
struct PathDialog {
    action: PathAction,
    path: String,
}

pub struct NotepadApp {
    storage: Option<Storage>,
    notebook: Notebook,
    selected_id: Option<String>,
    title: String,
    content: String,
    dirty: bool,
    last_edit: Option<Instant>,
    query: String,
    show_trash: bool,
    error: Option<String>,
    status: String,
    focus_search: bool,
    focus_title: bool,
    has_cjk_font: bool,
    notice: Option<String>,
    confirm_exit: bool,
    discard_on_exit: bool,
    pending_input: Vec<egui::Event>,
    close_after_input: bool,
    settle_editor_focus: bool,
    check_cjk_font: bool,
    #[cfg(target_os = "linux")]
    path_dialog: Option<PathDialog>,
}

impl NotepadApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let has_cjk_font = install_system_font(&cc.egui_ctx);
        let mut app = Self::from_storage(Storage::open_default());
        app.has_cjk_font = has_cjk_font;
        app.check_cjk_font = true;
        app
    }

    fn from_storage(result: Result<(Storage, Notebook), crate::storage::StorageError>) -> Self {
        let (storage, notebook, error) = match result {
            Ok((storage, notebook)) => (Some(storage), notebook, None),
            Err(error) => (
                None,
                Notebook::default(),
                Some(format!("无法打开笔记文件：{error}")),
            ),
        };
        let mut app = Self {
            storage,
            notebook,
            selected_id: None,
            title: String::new(),
            content: String::new(),
            dirty: false,
            last_edit: None,
            query: String::new(),
            show_trash: false,
            error,
            status: "所有更改已保存到本机".into(),
            focus_search: false,
            focus_title: false,
            has_cjk_font: true,
            notice: None,
            confirm_exit: false,
            discard_on_exit: false,
            pending_input: Vec::new(),
            close_after_input: false,
            settle_editor_focus: false,
            check_cjk_font: false,
            #[cfg(target_os = "linux")]
            path_dialog: None,
        };
        app.select_first();
        app
    }

    fn visible_notes(&self) -> Vec<&Note> {
        let query = self.query.trim().to_lowercase();
        let mut notes: Vec<_> = self
            .notebook
            .notes
            .iter()
            .filter(|note| {
                note.is_deleted == self.show_trash
                    && (query.is_empty()
                        || note.title.to_lowercase().contains(&query)
                        || note.content.to_lowercase().contains(&query))
            })
            .collect();
        notes.sort_by_cached_key(|note| {
            (
                std::cmp::Reverse(note.is_pinned),
                std::cmp::Reverse(chrono::DateTime::parse_from_rfc3339(&note.updated_at).ok()),
                note.id.clone(),
            )
        });
        notes
    }

    fn visible_ids(&self) -> Vec<String> {
        self.visible_notes()
            .into_iter()
            .map(|note| note.id.clone())
            .collect()
    }

    fn select_first(&mut self) {
        let next = self.visible_ids().first().cloned();
        self.load_selection(next);
    }

    fn load_selection(&mut self, id: Option<String>) {
        self.selected_id = id;
        let note = self
            .selected_id
            .as_ref()
            .and_then(|id| self.notebook.notes.iter().find(|note| &note.id == id));
        self.title = note.map(|note| note.title.clone()).unwrap_or_default();
        self.content = note.map(|note| note.content.clone()).unwrap_or_default();
        self.dirty = false;
        self.last_edit = None;
    }

    fn select(&mut self, id: String) {
        if self.selected_id.as_ref() == Some(&id) || !self.save_current() {
            return;
        }
        self.load_selection(Some(id));
    }

    fn edited(&mut self) {
        self.dirty = true;
        self.last_edit = Some(Instant::now());
        self.status = "未保存".into();
    }

    // Keep the notebook and editor intact when persistence fails. Every operation
    // that can replace the current draft must pass through this save gate first.
    fn save_current(&mut self) -> bool {
        if !self.dirty {
            return self.storage.is_some();
        }
        let mut candidate = self.notebook.clone();
        let Some(note) = self
            .selected_id
            .as_ref()
            .and_then(|id| candidate.notes.iter_mut().find(|note| &note.id == id))
        else {
            self.error = Some("当前笔记不存在；修改仍保留在编辑器中。".into());
            return false;
        };
        note.update(self.title.clone(), self.content.clone());
        if !self.persist(candidate) {
            return false;
        }
        self.dirty = false;
        self.last_edit = None;
        self.status = "所有更改已保存到本机".into();
        true
    }

    fn persist(&mut self, candidate: Notebook) -> bool {
        let Some(storage) = self.storage.as_mut() else {
            self.error = Some("笔记文件尚未打开，不能保存。".into());
            return false;
        };
        match storage.save(&candidate) {
            Ok(()) => {
                self.notebook = candidate;
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(format!(
                    "保存失败：{error}。修改仍在编辑器中，请重试或导出备份。"
                ));
                // Avoid retrying the same failed write on every animation frame.
                self.last_edit = None;
                false
            }
        }
    }

    fn new_note(&mut self) {
        if !self.save_current() {
            return;
        }
        let mut candidate = self.notebook.clone();
        let note = Note::new(String::new(), String::new());
        let id = note.id.clone();
        candidate.notes.push(note);
        if self.persist(candidate) {
            self.show_trash = false;
            self.query.clear();
            self.load_selection(Some(id));
            self.focus_title = true;
            self.status = "新笔记已创建".into();
        }
    }

    fn toggle_trash(&mut self, show: bool) {
        if show != self.show_trash && self.save_current() {
            self.show_trash = show;
            self.select_first();
        }
    }

    fn set_deleted(&mut self, deleted: bool) {
        if !self.save_current() {
            return;
        }
        let mut candidate = self.notebook.clone();
        let Some(note) = self
            .selected_id
            .as_ref()
            .and_then(|id| candidate.notes.iter_mut().find(|note| &note.id == id))
        else {
            return;
        };
        note.set_deleted(deleted);
        if self.persist(candidate) {
            self.select_first();
            self.status = if deleted {
                "已移入回收站，可随时恢复"
            } else {
                "已恢复笔记"
            }
            .into();
        }
    }

    fn toggle_pin(&mut self) {
        if !self.save_current() {
            return;
        }
        let mut candidate = self.notebook.clone();
        let Some(note) = self
            .selected_id
            .as_ref()
            .and_then(|id| candidate.notes.iter_mut().find(|note| &note.id == id))
        else {
            return;
        };
        note.set_pinned(!note.is_pinned);
        self.persist(candidate);
    }

    fn import_notes(&mut self) {
        if !self.save_current() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("导入笔记（复制导入，不覆盖已有笔记）")
            .add_filter(
                "笔记和备份",
                &["txt", "md", "markdown", "html", "htm", "json"],
            )
            .pick_file()
        else {
            self.file_dialog_dismissed();
            return;
        };
        self.import_from_path(&path);
    }

    fn import_from_path(&mut self, path: &Path) -> bool {
        if !self.save_current() {
            return false;
        }
        let mut candidate = self.notebook.clone();
        let prior_ids: std::collections::HashSet<_> =
            candidate.notes.iter().map(|note| note.id.clone()).collect();
        match import_file(path, &mut candidate, ImportMode::Copy) {
            Ok(report) => {
                let first_imported = candidate
                    .notes
                    .iter()
                    .find(|note| !note.is_deleted && !prior_ids.contains(&note.id))
                    .map(|note| note.id.clone());
                if self.persist(candidate) {
                    self.status = format!("已导入 {} 篇笔记", report.added);
                    self.notice = if report.warnings.is_empty() {
                        None
                    } else {
                        Some(report.warnings.join("\n"))
                    };
                    self.show_trash = false;
                    self.query.clear();
                    if let Some(id) = first_imported {
                        self.load_selection(Some(id));
                    } else {
                        self.select_first();
                    }
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.error = Some(format!("导入失败：{error}。原有笔记未改变。"));
                false
            }
        }
    }

    fn snapshot_with_draft(&self) -> Notebook {
        let mut snapshot = self.notebook.clone();
        if self.dirty {
            if let Some(note) = self
                .selected_id
                .as_ref()
                .and_then(|id| snapshot.notes.iter_mut().find(|note| &note.id == id))
            {
                note.update(self.title.clone(), self.content.clone());
            }
        }
        snapshot
    }

    fn export_json(&mut self) {
        let filename = format!(
            "nebulabook-backup-{}.json",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出完整 JSON 备份")
            .add_filter("JSON 备份", &["json"])
            .set_file_name(filename)
            .save_file()
        else {
            self.file_dialog_dismissed();
            return;
        };
        self.export_json_to_path(&path);
    }

    fn export_json_to_path(&mut self, path: &Path) -> bool {
        match export_backup(path, &self.snapshot_with_draft()) {
            Ok(()) => {
                self.notice = Some(format!(
                    "备份已导出到 {}，包含当前未保存的修改。",
                    path.display()
                ));
                true
            }
            Err(error) => {
                self.error = Some(format!("导出失败：{error}"));
                false
            }
        }
    }

    fn export_selected(&mut self, markdown: bool) {
        if self.selected_id.is_none() {
            return;
        }
        let extension = if markdown { "md" } else { "txt" };
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出当前笔记")
            .add_filter("文本笔记", &[extension])
            .set_file_name(format!("nebulabook-note.{extension}"))
            .save_file()
        else {
            self.file_dialog_dismissed();
            return;
        };
        self.export_selected_to_path(&path, markdown);
    }

    fn export_selected_to_path(&mut self, path: &Path, markdown: bool) -> bool {
        let snapshot = self.snapshot_with_draft();
        let Some(note) = self
            .selected_id
            .as_ref()
            .and_then(|id| snapshot.notes.iter().find(|note| &note.id == id))
        else {
            self.error = Some("请先选择需要导出的笔记。".into());
            return false;
        };
        let format = if markdown {
            NoteFormat::Markdown
        } else {
            NoteFormat::Text
        };
        match export_note(path, note, format) {
            Ok(()) => {
                self.notice = Some(format!("笔记已导出到 {}。", path.display()));
                true
            }
            Err(error) => {
                self.error = Some(format!("导出失败：{error}"));
                false
            }
        }
    }

    fn file_dialog_dismissed(&mut self) {
        // rfd cannot distinguish cancellation from a missing portal/zenity.
        // Never claim an error or automatically perform an alternate operation.
        #[cfg(target_os = "linux")]
        {
            self.notice = Some(
                "未选择文件。若系统文件选择器未能打开，可在导入 / 导出菜单中使用“直接输入路径”。"
                    .into(),
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn open_path_dialog(&mut self, action: PathAction) {
        self.path_dialog = Some(PathDialog {
            action,
            path: String::new(),
        });
    }

    #[cfg(target_os = "linux")]
    fn submit_path(&mut self, action: PathAction, path: &str) -> bool {
        let path = Path::new(path);
        if !path.is_absolute() {
            self.error = Some("请输入以 / 开头的完整文件路径；不展开 ~、环境变量或命令。".into());
            return false;
        }
        match action {
            PathAction::Import => self.import_from_path(path),
            PathAction::Backup => self.export_json_to_path(path),
            PathAction::Note(markdown) => self.export_selected_to_path(path, markdown),
        }
    }

    #[cfg(target_os = "linux")]
    fn manual_path_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.path_dialog.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("manual-file-path")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.heading(match dialog.action {
                PathAction::Import => "直接输入导入文件路径",
                PathAction::Backup => "直接输入 JSON 备份路径",
                PathAction::Note(false) => "直接输入 TXT 导出路径",
                PathAction::Note(true) => "直接输入 Markdown 导出路径",
            });
            ui.label("无需桌面 portal。请输入完整路径，例如 /home/me/Documents/notes.json。");
            ui.label("导入先验证再复制；导出只创建新文件，不覆盖已有文件。");
            ui.add(egui::TextEdit::singleline(&mut dialog.path).desired_width(520.0));
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::DARK_RED, error);
            }
            ui.horizontal(|ui| {
                submit = ui.button("确认路径并执行").clicked();
                cancel = ui.button("取消").clicked();
            });
        });
        if !(cancel
            || response.should_close()
            || (submit && self.submit_path(dialog.action, &dialog.path)))
        {
            self.path_dialog = Some(dialog);
        }
    }

    fn can_close(&mut self) -> bool {
        self.discard_on_exit || !self.dirty || self.save_current()
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        if let Some(edited) = self.last_edit {
            let elapsed = edited.elapsed();
            if elapsed >= AUTOSAVE_DELAY {
                self.save_current();
            } else {
                ctx.request_repaint_after(AUTOSAVE_DELAY - elapsed);
            }
        }
    }

    fn exit_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_exit {
            return;
        }
        egui::Window::new("尚有未保存的修改")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("保存未成功。你可以重试，或先导出包含当前修改的 JSON 备份。");
                ui.horizontal(|ui| {
                    if ui.button("返回编辑").clicked() {
                        self.confirm_exit = false;
                    }
                    if ui.button("导出备份…").clicked() {
                        self.export_json();
                    }
                    #[cfg(target_os = "linux")]
                    if ui.button("输入备份路径…").clicked() {
                        self.open_path_dialog(PathAction::Backup);
                    }
                    if ui.button("重试保存并退出").clicked() && self.save_current() {
                        self.confirm_exit = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.separator();
                if ui.button("放弃未保存的修改并退出").clicked() {
                    self.discard_on_exit = true;
                    self.confirm_exit = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) -> (bool, bool) {
        #[cfg(target_os = "linux")]
        if self.path_dialog.is_some() {
            return (false, false);
        }
        let save = ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        let new = ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::N));
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_search = true;
        }
        (save, new)
    }

    fn order_input(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let mut events = std::mem::take(&mut self.pending_input);
        events.append(&mut input.events);
        if input.viewport().visible() == Some(false) {
            // eframe retains raw input without a UI pass while hidden. Do not
            // split it here: another hidden tick would otherwise put our later
            // queued chunk ahead of the earlier input retained by eframe.
            input.events = events;
            return;
        }
        if std::mem::take(&mut self.settle_editor_focus) && !events.is_empty() {
            // egui installs a TextEdit's navigation filter only after it also
            // had focus in the previous pass. Let a newly clicked editor settle
            // before replaying input, so its first Tab indents rather than leaves.
            self.pending_input = events;
            ctx.request_repaint();
            return;
        }
        let mut editing_event_seen = false;
        let mut focus_change_seen = false;
        let split = events.iter().position(|event| {
            let focus_change = matches!(event, egui::Event::PointerButton { pressed: true, .. })
                || matches!(event, egui::Event::Key { key, pressed: true, modifiers, .. }
                    if *key == egui::Key::Tab || *key == egui::Key::Escape
                        || (modifiers.command && matches!(key, egui::Key::F | egui::Key::N | egui::Key::S)));
            let editing_event = matches!(event,
                egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut
                    | egui::Event::Ime(_) | egui::Event::Key { .. });
            if (editing_event_seen && focus_change)
                || (focus_change_seen && (editing_event || focus_change)) { return true; }
            editing_event_seen |= editing_event && !focus_change;
            focus_change_seen |= focus_change;
            false
        });
        if let Some(split) = split {
            // egui treats pointer focus changes before TextEdit's event loop.
            // Apply each focus change before another focus/navigation event, too:
            // a click into the body must take effect before its Tab key filter.
            // Render earlier keyboard/IME events first, then replay the remainder
            // in order on the next repaint through egui itself (no custom editing).
            self.pending_input = events.split_off(split);
            ctx.request_repaint();
        }
        input.events = events;
    }

    fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.check_cjk_font) {
            self.has_cjk_font = ctx.fonts_mut(|fonts| {
                fonts.has_glyphs(&egui::FontId::proportional(16.0), "中文记事本保存回收站")
                    && fonts.has_glyphs(&egui::FontId::monospace(16.0), "中文记事本保存回收站")
            });
            if crate::runtime::option("NO_ERROR_DIALOG").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "Nebulabook CJK font initialized: {}",
                    self.has_cjk_font
                );
            }
        }
        let (save, new) = self.shortcuts(&ctx);
        self.render(ui);
        if save {
            self.save_current();
        }
        if new {
            self.new_note();
        }
        if ctx.input(|input| input.viewport().close_requested()) || self.close_after_input {
            if !self.pending_input.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_after_input = true;
            } else {
                let deferred = std::mem::take(&mut self.close_after_input);
                if !self.can_close() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.confirm_exit = true;
                } else if deferred {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if !self.pending_input.is_empty() {
            ctx.request_repaint();
        }
        self.exit_dialog(&ctx);
        #[cfg(target_os = "linux")]
        self.manual_path_dialog(&ctx);
        self.autosave(&ctx);
    }

    fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::NewNote => self.new_note(),
            UiAction::Save => {
                self.save_current();
            }
            UiAction::Select(id) => self.select(id),
            UiAction::ShowTrash(show) => self.toggle_trash(show),
            UiAction::SetDeleted(deleted) => self.set_deleted(deleted),
            UiAction::TogglePin => self.toggle_pin(),
            UiAction::Import => self.import_notes(),
            UiAction::ExportBackup => self.export_json(),
            UiAction::ExportNote(markdown) => self.export_selected(markdown),
            #[cfg(target_os = "linux")]
            UiAction::ManualPath(action) => self.open_path_dialog(action),
            UiAction::Reopen => {
                let has_cjk_font = self.has_cjk_font;
                *self = Self::from_storage(Storage::open_default());
                self.has_cjk_font = has_cjk_font;
            }
        }
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        // Buttons are rendered before the editor. Defer mutations and snapshots
        // until its TextEdit has consumed all text/IME events in this frame.
        let mut actions = Vec::new();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Nebulabook");
                ui.separator();
                ui.add_enabled_ui(self.storage.is_some(), |ui| {
                    if ui.button("＋ 新建").on_hover_text("Ctrl+N / ⌘N").clicked() {
                        actions.push(UiAction::NewNote);
                    }
                    if ui
                        .add_enabled(self.selected_id.is_some(), egui::Button::new("保存"))
                        .on_hover_text("Ctrl+S / ⌘S")
                        .clicked()
                    {
                        actions.push(UiAction::Save);
                    }
                    ui.menu_button("导入 / 导出", |ui| {
                        if ui.button("导入 TXT / Markdown / HTML / JSON…").clicked() {
                            ui.close();
                            actions.push(UiAction::Import);
                        }
                        ui.separator();
                        if ui.button("导出全部为 JSON 备份…").clicked() {
                            ui.close();
                            actions.push(UiAction::ExportBackup);
                        }
                        ui.add_enabled_ui(self.selected_id.is_some(), |ui| {
                            if ui.button("导出当前笔记为 TXT…").clicked() {
                                ui.close();
                                actions.push(UiAction::ExportNote(false));
                            }
                            if ui.button("导出当前笔记为 Markdown…").clicked() {
                                ui.close();
                                actions.push(UiAction::ExportNote(true));
                            }
                        });
                        #[cfg(target_os = "linux")]
                        {
                            ui.separator();
                            ui.menu_button("直接输入路径（无需 portal）", |ui| {
                                for (label, action) in [
                                    ("导入文件…", PathAction::Import),
                                    ("导出 JSON 备份…", PathAction::Backup),
                                    ("导出当前笔记 TXT…", PathAction::Note(false)),
                                    ("导出当前笔记 Markdown…", PathAction::Note(true)),
                                ] {
                                    let enabled = !matches!(action, PathAction::Note(_))
                                        || self.selected_id.is_some();
                                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                        ui.close();
                                        actions.push(UiAction::ManualPath(action));
                                    }
                                }
                            });
                        }
                        ui.separator();
                        ui.label("HTML 导入为纯文本；原始内容保留在备份中。");
                        ui.label("导出请选择新文件名，不会覆盖已有文件。");
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("本地文件 · 离线使用");
                });
            });
            if !self.has_cjk_font {
                ui.colored_label(
                    egui::Color32::DARK_RED,
                    "Chinese font missing. Install Noto Sans CJK, then restart Nebulabook.",
                );
            }
            if let Some(notice) = self.notice.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    if ui.small_button("关闭提示").clicked() {
                        self.notice = None;
                    }
                });
            }
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                    if self.storage.is_none() && ui.button("重试打开").clicked() {
                        actions.push(UiAction::Reopen);
                    } else if self.storage.is_some()
                        && self.dirty
                        && ui.button("重试保存").clicked()
                    {
                        actions.push(UiAction::Save);
                    }
                });
            }
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.dirty {
                    ui.colored_label(egui::Color32::from_rgb(170, 100, 10), "● 未保存");
                } else {
                    ui.label(&self.status);
                }
                ui.separator();
                ui.label(format!(
                    "{} 字符 · {} 行",
                    self.content.chars().count(),
                    self.content.lines().count().max(1)
                ));
                if let Some(storage) = &self.storage {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label("数据文件")
                            .on_hover_text(storage.path().display().to_string());
                    });
                }
            });
        });

        egui::Panel::left("notes")
            .default_size(245.0)
            .min_size(180.0)
            .max_size(420.0)
            .show(ui, |ui| {
                let search_id = ui.make_persistent_id("note-search");
                if self.focus_search {
                    ui.memory_mut(|memory| memory.request_focus(search_id));
                    self.focus_search = false;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(search_id)
                        .hint_text("搜索标题和正文…")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.show_trash, "笔记").clicked() {
                        actions.push(UiAction::ShowTrash(false));
                    }
                    if ui.selectable_label(self.show_trash, "回收站").clicked() {
                        actions.push(UiAction::ShowTrash(true));
                    }
                });
                ui.separator();
                let notes: Vec<_> = self
                    .visible_notes()
                    .into_iter()
                    .map(|note| (note.id.clone(), note.title.clone(), note.is_pinned))
                    .collect();
                ui.label(format!("{} 篇", notes.len()));
                egui::ScrollArea::vertical().show_rows(ui, 34.0, notes.len(), |ui, rows| {
                    for (id, title, pinned) in &notes[rows] {
                        let title = if title.trim().is_empty() {
                            "无标题笔记"
                        } else {
                            title.trim()
                        };
                        let label = if *pinned {
                            format!("↑ {title}")
                        } else {
                            title.to_string()
                        };
                        let selected = self.selected_id.as_ref() == Some(id);
                        let response = ui.add_sized(
                            [ui.available_width(), 34.0],
                            egui::Button::new(label).truncate().selected(selected),
                        );
                        if response.clicked() {
                            actions.push(UiAction::Select(id.clone()));
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.storage.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("请先解决上方文件错误。原有数据不会被覆盖。");
                });
                return;
            }
            let Some(id) = self.selected_id.clone() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(70.0);
                    ui.heading(if self.show_trash {
                        "回收站为空"
                    } else {
                        "写下第一个想法"
                    });
                    if !self.show_trash && ui.button("新建笔记").clicked() {
                        actions.push(UiAction::NewNote);
                    }
                });
                return;
            };
            let note = self
                .notebook
                .notes
                .iter()
                .find(|note| note.id == id)
                .unwrap();
            let deleted = note.is_deleted;
            let pinned = note.is_pinned;
            ui.horizontal(|ui| {
                if deleted {
                    ui.label("回收站中的笔记只读");
                    if ui.button("恢复笔记").clicked() {
                        actions.push(UiAction::SetDeleted(false));
                    }
                } else {
                    if ui
                        .button(if pinned { "取消置顶" } else { "置顶" })
                        .clicked()
                    {
                        actions.push(UiAction::TogglePin);
                    }
                    if ui.button("移入回收站").clicked() {
                        actions.push(UiAction::SetDeleted(true));
                    }
                }
            });
            ui.add_enabled_ui(!deleted, |ui| {
                let title_id = ui.make_persistent_id(("note-title", &id));
                if self.focus_title {
                    ui.memory_mut(|memory| memory.request_focus(title_id));
                    self.focus_title = false;
                }
                let title = ui.add(
                    egui::TextEdit::singleline(&mut self.title)
                        .id(title_id)
                        .hint_text("笔记标题")
                        .font(egui::TextStyle::Heading)
                        .desired_width(f32::INFINITY),
                );
                if title.changed() {
                    self.edited();
                }
                if title.lost_focus() {
                    // egui moves focus before TextEdit handles events, but leaves
                    // the navigation Tab for the newly focused multiline editor.
                    // Consume only that first Tab, not indentation after a click
                    // or subsequent Tabs pressed after entering the body.
                    ui.input_mut(|input| {
                        if let Some(index) = input.events.iter().position(|event| {
                            matches!(event, egui::Event::PointerButton { pressed: true, .. })
                                || matches!(
                                    event,
                                    egui::Event::Key {
                                        key: egui::Key::Tab,
                                        pressed: true,
                                        ..
                                    }
                                )
                        }) {
                            if matches!(&input.events[index], egui::Event::Key {
                                key: egui::Key::Tab, pressed: true, modifiers, ..
                            } if !modifiers.shift)
                            {
                                input.events.remove(index);
                            }
                        }
                    });
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt(("editor-scroll", &id))
                    .show(ui, |ui| {
                        let response = ui.add_sized(
                            [ui.available_width(), ui.available_height().max(200.0)],
                            egui::TextEdit::multiline(&mut self.content)
                                .id_salt(("note-content", &id))
                                .hint_text("开始书写…")
                                .font(egui::TextStyle::Body)
                                .desired_width(f32::INFINITY)
                                .lock_focus(true),
                        );
                        if response.gained_focus() {
                            self.settle_editor_focus = true;
                            ui.ctx().request_repaint();
                        }
                        if response.changed() {
                            self.edited();
                        }
                    });
            });
        });
        for action in actions {
            self.apply_action(action);
        }
    }
}

impl eframe::App for NotepadApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.order_input(ctx, input);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Hidden logic sees fresh window state but stale UI events. Always defer
        // a hidden close until an actual UI pass has consumed retained input.
        let hidden_close = ctx.input(|input| {
            input.viewport().visible() == Some(false)
                && (input.viewport().close_requested() || self.close_after_input)
        });
        if hidden_close {
            self.close_after_input = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
            return;
        }
        // Keep scheduled saves alive while hidden without touching UI input.
        self.autosave(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn app() -> (TempDir, NotepadApp) {
        let directory = TempDir::new().unwrap();
        let app = NotepadApp::from_storage(Storage::open(directory.path().join("notes.json")));
        assert!(app.storage.is_some());
        (directory, app)
    }

    fn fail_future_saves(app: &NotepadApp) {
        let path = app.storage.as_ref().unwrap().path();
        std::fs::remove_file(path).unwrap();
        std::fs::create_dir(path).unwrap();
    }

    fn run_headless(
        ctx: &egui::Context,
        input: egui::RawInput,
        render: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(input, render);
        // These tests inspect shapes and application state without a GPU renderer.
        // egui 0.36 requires explicitly acknowledging unused texture updates.
        output.textures_delta.clear();
        output
    }

    fn render_input(
        app: &mut NotepadApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut raw);
        let mut output = run_headless(ctx, raw, |ctx| app.frame(ctx));
        while !app.pending_input.is_empty() {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                ..Default::default()
            };
            eframe::App::raw_input_hook(app, ctx, &mut raw);
            output = run_headless(ctx, raw, |ctx| app.frame(ctx));
        }
        output
    }

    fn click_at(position: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    }

    fn label_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.job.text == label {
                        return Some(text.pos + text.galley.size() * 0.5);
                    }
                }
                None
            })
            .unwrap_or_else(|| panic!("Missing clickable label: {label}"))
    }

    fn focus_body_and_type(app: &mut NotepadApp, ctx: &egui::Context) {
        app.focus_title = false;
        render_input(app, ctx, Vec::new());
        render_input(app, ctx, click_at(egui::pos2(400.0, 180.0)));
        render_input(app, ctx, vec![egui::Event::Text(" FIRST".into())]);
        assert_eq!(app.content, "before FIRST");
    }

    fn key_event(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn keyboard_new_title_tab_body_and_save_preserve_exact_text() {
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        for mode in ["staged", "single_batch", "one_event_per_frame"] {
            let (_directory, mut app) = app();
            let ctx = egui::Context::default();
            render_input(&mut app, &ctx, Vec::new());
            let batches = vec![
                vec![
                    key_event(egui::Key::N, true, command),
                    key_event(egui::Key::N, false, command),
                ],
                vec![],
                vec![egui::Event::Text("Linux smoke title".into())],
                vec![
                    key_event(egui::Key::Tab, true, egui::Modifiers::NONE),
                    key_event(egui::Key::Tab, false, egui::Modifiers::NONE),
                ],
                vec![egui::Event::Text("Linux smoke body 123".into())],
                vec![
                    key_event(egui::Key::S, true, command),
                    key_event(egui::Key::S, false, command),
                ],
            ];
            if mode == "single_batch" {
                render_input(&mut app, &ctx, batches.into_iter().flatten().collect());
            } else if mode == "one_event_per_frame" {
                for event in batches.into_iter().flatten() {
                    if let egui::Event::Text(text) = event {
                        for character in text.chars() {
                            render_input(
                                &mut app,
                                &ctx,
                                vec![egui::Event::Text(character.to_string())],
                            );
                        }
                    } else {
                        render_input(&mut app, &ctx, vec![event]);
                    }
                }
            } else {
                for events in batches {
                    render_input(&mut app, &ctx, events);
                }
            }
            assert_eq!(app.title, "Linux smoke title", "mode={mode}");
            assert_eq!(app.content, "Linux smoke body 123", "mode={mode}");
            let saved: Notebook = serde_json::from_slice(
                &std::fs::read(app.storage.as_ref().unwrap().path()).unwrap(),
            )
            .unwrap();
            assert_eq!(saved.notes.len(), 1, "mode={mode}");
            assert_eq!(saved.notes[0].title, "Linux smoke title", "mode={mode}");
            assert_eq!(
                saved.notes[0].content, "Linux smoke body 123",
                "mode={mode}"
            );
        }
    }

    #[test]
    fn title_tab_navigates_once_then_body_tabs_indent_and_unindent() {
        let (_directory, mut app) = app();
        app.new_note();
        let ctx = egui::Context::default();
        render_input(&mut app, &ctx, Vec::new());
        render_input(
            &mut app,
            &ctx,
            vec![
                egui::Event::Text("title".into()),
                key_event(egui::Key::Tab, true, egui::Modifiers::NONE),
                key_event(egui::Key::Tab, false, egui::Modifiers::NONE),
                key_event(egui::Key::Tab, true, egui::Modifiers::NONE),
                key_event(egui::Key::Tab, false, egui::Modifiers::NONE),
                egui::Event::Text("body".into()),
            ],
        );
        assert_eq!(app.title, "title");
        assert_eq!(app.content, "\tbody");
        let body_focus = ctx.memory(|memory| memory.focused());
        render_input(
            &mut app,
            &ctx,
            vec![key_event(egui::Key::Tab, true, egui::Modifiers::SHIFT)],
        );
        assert_eq!(app.content, "body");
        assert_eq!(ctx.memory(|memory| memory.focused()), body_focus);
        render_input(
            &mut app,
            &ctx,
            vec![key_event(egui::Key::Tab, true, egui::Modifiers::NONE)],
        );
        assert_eq!(app.content, "body\t");
        assert_eq!(ctx.memory(|memory| memory.focused()), body_focus);
    }

    #[test]
    fn clicking_body_then_tab_in_same_batch_keeps_intentional_indentation() {
        let (_directory, mut app) = app();
        app.new_note();
        let ctx = egui::Context::default();
        render_input(&mut app, &ctx, Vec::new());
        let mut events = click_at(egui::pos2(400.0, 180.0));
        events.extend([
            key_event(egui::Key::Tab, true, egui::Modifiers::NONE),
            key_event(egui::Key::Tab, false, egui::Modifiers::NONE),
            egui::Event::Text("clicked body".into()),
        ]);
        render_input(&mut app, &ctx, events);
        assert!(app.title.is_empty());
        assert_eq!(app.content, "\tclicked body");
    }

    #[test]
    fn title_shift_tab_does_not_change_editor_text() {
        let (_directory, mut app) = app();
        app.new_note();
        app.title = "title".into();
        app.content = "\tbody".into();
        let ctx = egui::Context::default();
        let output = render_input(&mut app, &ctx, Vec::new());
        let title_position = label_position(&output, "title");
        let title_focus = ctx.memory(|memory| memory.focused());
        render_input(
            &mut app,
            &ctx,
            vec![key_event(egui::Key::Tab, true, egui::Modifiers::SHIFT)],
        );
        render_input(&mut app, &ctx, Vec::new());
        assert_ne!(ctx.memory(|memory| memory.focused()), title_focus);
        assert_eq!(app.title, "title");
        assert_eq!(app.content, "\tbody");
        render_input(&mut app, &ctx, click_at(title_position));
        render_input(&mut app, &ctx, vec![egui::Event::Text("X".into())]);
        assert_eq!(app.title.len(), "titleX".len());
        assert!(app.title.contains('X'));
        assert_eq!(app.content, "\tbody");
    }

    #[test]
    fn same_frame_text_then_new_click_saves_final_input_to_original_note() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        render_input(&mut app, &ctx, events);
        assert_eq!(app.notebook.notes.len(), 2);
        assert_ne!(app.selected_id.as_ref(), Some(&original));
        assert!(app.content.is_empty());
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST LAST"
        );
    }

    #[test]
    fn same_frame_text_then_note_selection_saves_before_switching_draft() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.title = "Original note".into();
        app.content = "before".into();
        app.edited();
        app.new_note();
        let target = app.selected_id.clone().unwrap();
        app.title = "Target note".into();
        app.content = "target body".into();
        app.edited();
        app.save_current();
        app.select(original.clone());
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "Target note")));
        render_input(&mut app, &ctx, events);
        assert_eq!(app.selected_id.as_ref(), Some(&target));
        assert_eq!(app.content, "target body");
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST LAST"
        );
    }

    #[test]
    fn same_frame_text_precedes_save_pin_delete_and_trash_actions() {
        for label in ["保存", "置顶", "移入回收站", "回收站"] {
            let (_directory, mut app) = app();
            app.new_note();
            app.content = "before".into();
            app.edited();
            app.save_current();
            let ctx = egui::Context::default();
            focus_body_and_type(&mut app, &ctx);
            let output = render_input(&mut app, &ctx, Vec::new());
            let mut events = vec![egui::Event::Text(" LAST".into())];
            events.extend(click_at(label_position(&output, label)));
            render_input(&mut app, &ctx, events);
            assert_eq!(
                app.notebook.notes[0].content, "before FIRST LAST",
                "action: {label}"
            );
            match label {
                "置顶" => assert!(app.notebook.notes[0].is_pinned),
                "移入回收站" => assert!(app.notebook.notes[0].is_deleted),
                "回收站" => assert!(app.show_trash),
                _ => assert!(!app.dirty),
            }
        }
    }

    #[test]
    fn same_frame_body_text_then_title_focus_keeps_text_in_its_original_field() {
        let (_directory, mut app) = app();
        app.new_note();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "笔记标题")));
        events.push(egui::Event::Text("New title".into()));
        render_input(&mut app, &ctx, events);
        assert_eq!(app.content, "before FIRST LAST");
        assert_eq!(app.title, "New title");
    }

    #[test]
    fn same_frame_ime_commit_before_new_click_is_saved_without_duplication() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "中文".into(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),
        ];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        render_input(&mut app, &ctx, events);
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST中文"
        );
        assert!(app.content.is_empty());
    }

    #[test]
    fn same_batch_new_click_then_text_types_into_new_note_only() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        events.push(egui::Event::Text("New title".into()));
        render_input(&mut app, &ctx, events);
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST LAST"
        );
        assert_ne!(app.selected_id.as_ref(), Some(&original));
        assert_eq!(app.title, "New title");
        assert!(app.content.is_empty());
    }

    #[test]
    fn pending_text_is_preserved_before_save_new_and_search_shortcuts() {
        for key in [egui::Key::S, egui::Key::N, egui::Key::F] {
            let (_directory, mut app) = app();
            app.new_note();
            let original = app.selected_id.clone().unwrap();
            app.content = "before".into();
            app.edited();
            app.save_current();
            let ctx = egui::Context::default();
            focus_body_and_type(&mut app, &ctx);
            let mut events = vec![
                egui::Event::Text(" LAST".into()),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
            ];
            if key == egui::Key::F {
                events.push(egui::Event::Text("needle".into()));
            }
            render_input(&mut app, &ctx, events);
            if key == egui::Key::F {
                assert_eq!(app.content, "before FIRST LAST");
                assert_eq!(app.query, "needle");
            } else {
                assert_eq!(
                    app.notebook
                        .notes
                        .iter()
                        .find(|note| note.id == original)
                        .unwrap()
                        .content,
                    "before FIRST LAST"
                );
            }
            assert_eq!(
                app.notebook.notes.len(),
                if key == egui::Key::N { 2 } else { 1 }
            );
        }
    }

    #[test]
    fn close_request_waits_for_deferred_input_and_persists_the_final_text() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
        let output = run_headless(&ctx, raw, |ctx| app.frame(ctx));
        assert!(app.close_after_input);
        assert!(!app.pending_input.is_empty());
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        let output = render_input(&mut app, &ctx, Vec::new());
        assert!(app.pending_input.is_empty());
        assert!(!app.close_after_input);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST LAST"
        );
    }

    fn hidden_close_input(events: Vec<egui::Event>, minimized: bool) -> egui::RawInput {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let viewport = raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.minimized = Some(minimized);
        viewport.occluded = Some(!minimized);
        viewport.events.push(egui::ViewportEvent::Close);
        raw
    }

    fn hidden_tick(
        app: &mut NotepadApp,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
    ) -> egui::LogicOutput {
        eframe::App::raw_input_hook(app, ctx, raw);
        ctx.run_logic(raw, |ctx| {
            eframe::App::logic(app, ctx, &mut eframe::Frame::_new_kittest());
        })
    }

    fn restore_retained_input(
        app: &mut NotepadApp,
        ctx: &egui::Context,
        mut raw: egui::RawInput,
    ) -> egui::FullOutput {
        let viewport = raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.minimized = Some(false);
        viewport.occluded = Some(false);
        viewport.events.clear();
        eframe::App::raw_input_hook(app, ctx, &mut raw);
        let mut output = run_headless(ctx, raw, |ui| app.frame(ui));
        if !app.pending_input.is_empty() {
            output = render_input(app, ctx, Vec::new());
        }
        output
    }

    fn assert_hidden_close_deferred(app: &NotepadApp, output: &egui::LogicOutput) {
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
        assert!(commands.contains(&egui::ViewportCommand::Visible(true)));
        assert!(commands.contains(&egui::ViewportCommand::Minimized(false)));
        assert!(commands.contains(&egui::ViewportCommand::Focus));
        assert!(!commands.contains(&egui::ViewportCommand::Close));
        assert!(app.close_after_input);
    }

    #[test]
    fn hidden_close_before_autosave_restores_then_saves_and_exits() {
        for minimized in [true, false] {
            let (_directory, mut app) = app();
            app.new_note();
            app.content = "draft before autosave deadline".into();
            app.edited();
            let ctx = egui::Context::default();
            let mut raw = hidden_close_input(Vec::new(), minimized);
            let output = hidden_tick(&mut app, &ctx, &mut raw);
            assert_hidden_close_deferred(&app, &output);
            assert!(app.dirty);
            assert!(app.notebook.notes[0].content.is_empty());
            let output = restore_retained_input(&mut app, &ctx, raw);
            assert!(!app.dirty);
            assert!(!app.close_after_input);
            assert_eq!(
                app.notebook.notes[0].content,
                "draft before autosave deadline"
            );
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close));
        }
    }

    #[test]
    fn hidden_close_save_failures_keep_draft_until_explicit_discard() {
        for external_conflict in [false, true] {
            let (_directory, mut app) = app();
            app.new_note();
            app.content = "protected unsaved draft".into();
            app.edited();
            if external_conflict {
                std::fs::write(app.storage.as_ref().unwrap().path(), "external writer").unwrap();
            } else {
                fail_future_saves(&app);
            }
            let ctx = egui::Context::default();
            let mut raw = hidden_close_input(Vec::new(), true);
            let output = hidden_tick(&mut app, &ctx, &mut raw);
            assert_hidden_close_deferred(&app, &output);
            let output = restore_retained_input(&mut app, &ctx, raw);
            assert!(app.dirty);
            assert!(app.confirm_exit);
            assert!(app.error.is_some());
            assert_eq!(app.content, "protected unsaved draft");
            assert!(app.notebook.notes[0].content.is_empty());
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert!(commands.contains(&egui::ViewportCommand::CancelClose));
            assert!(!commands.contains(&egui::ViewportCommand::Close));
            if external_conflict {
                assert_eq!(
                    std::fs::read_to_string(app.storage.as_ref().unwrap().path()).unwrap(),
                    "external writer"
                );
            }
            app.discard_on_exit = true;
            app.confirm_exit = false;
            let mut raw = hidden_close_input(Vec::new(), true);
            let output = hidden_tick(&mut app, &ctx, &mut raw);
            assert_hidden_close_deferred(&app, &output);
            let output = restore_retained_input(&mut app, &ctx, raw);
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close));
            assert_eq!(app.content, "protected unsaved draft");
        }
    }

    #[test]
    fn repeated_hidden_ticks_preserve_input_order_until_visible_close() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        events.push(egui::Event::Text("new title".into()));
        let mut raw = hidden_close_input(events.clone(), true);
        let output = hidden_tick(&mut app, &ctx, &mut raw);
        assert_hidden_close_deferred(&app, &output);
        assert_eq!(raw.events, events);
        assert!(app.pending_input.is_empty());
        assert_eq!(app.content, "before FIRST");
        events.push(egui::Event::Text(" end".into()));
        raw.events.push(egui::Event::Text(" end".into()));
        let output = hidden_tick(&mut app, &ctx, &mut raw);
        assert_hidden_close_deferred(&app, &output);
        assert_eq!(raw.events, events);
        assert!(app.pending_input.is_empty());
        let output = restore_retained_input(&mut app, &ctx, raw);
        assert_eq!(app.notebook.notes.len(), 2);
        assert_eq!(
            app.notebook
                .notes
                .iter()
                .find(|note| note.id == original)
                .unwrap()
                .content,
            "before FIRST LAST"
        );
        assert_eq!(app.title, "new title end");
        assert!(!app.dirty);
        assert!(app.pending_input.is_empty());
        assert!(!app.close_after_input);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
    }

    #[test]
    fn close_waits_for_clicked_editor_focus_and_queued_indentation() {
        let (_directory, mut app) = app();
        app.new_note();
        let ctx = egui::Context::default();
        render_input(&mut app, &ctx, Vec::new());
        let mut events = click_at(egui::pos2(400.0, 180.0));
        events.extend([
            key_event(egui::Key::Tab, true, egui::Modifiers::NONE),
            key_event(egui::Key::Tab, false, egui::Modifiers::NONE),
            egui::Event::Text("last input before close".into()),
        ]);
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
        let output = run_headless(&ctx, raw, |ui| app.frame(ui));
        assert!(app.settle_editor_focus);
        assert!(app.close_after_input);
        assert!(!app.pending_input.is_empty());
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        let output = render_input(&mut app, &ctx, Vec::new());
        assert!(app.pending_input.is_empty());
        assert!(!app.close_after_input);
        assert!(!app.dirty);
        assert_eq!(app.notebook.notes[0].content, "\tlast input before close");
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
    }

    #[test]
    fn same_frame_final_input_survives_failed_new_click_save() {
        let (_directory, mut app) = app();
        app.new_note();
        let original = app.selected_id.clone().unwrap();
        app.content = "before".into();
        app.edited();
        app.save_current();
        let ctx = egui::Context::default();
        focus_body_and_type(&mut app, &ctx);
        let output = render_input(&mut app, &ctx, Vec::new());
        fail_future_saves(&app);
        let mut events = vec![egui::Event::Text(" LAST".into())];
        events.extend(click_at(label_position(&output, "＋ 新建")));
        render_input(&mut app, &ctx, events);
        assert_eq!(app.selected_id.as_ref(), Some(&original));
        assert_eq!(app.notebook.notes.len(), 1);
        assert_eq!(app.content, "before FIRST LAST");
        assert!(app.dirty);
        assert!(app.error.is_some());
    }

    #[test]
    fn new_note_saves_dirty_draft_before_switching_identity() {
        let (_directory, mut app) = app();
        app.new_note();
        let first_id = app.selected_id.clone().unwrap();
        app.title = "第一篇".into();
        app.content = "最新正文".into();
        app.edited();
        app.new_note();
        assert_eq!(app.notebook.notes.len(), 2);
        assert_ne!(app.selected_id.as_ref(), Some(&first_id));
        let first = app
            .notebook
            .notes
            .iter()
            .find(|note| note.id == first_id)
            .unwrap();
        assert_eq!(first.title, "第一篇");
        assert_eq!(first.content, "最新正文");
        assert!(!app.dirty);
    }

    #[test]
    fn failed_save_keeps_draft_and_blocks_switch_new_delete_and_close() {
        let (_directory, mut app) = app();
        app.new_note();
        let first_id = app.selected_id.clone().unwrap();
        app.new_note();
        let second_id = app.selected_id.clone().unwrap();
        app.content = "不得丢失的修改".into();
        app.edited();
        fail_future_saves(&app);
        app.select(first_id);
        assert_eq!(app.selected_id.as_ref(), Some(&second_id));
        assert_eq!(app.content, "不得丢失的修改");
        assert!(app.dirty);
        assert!(app.error.is_some());
        assert!(app.last_edit.is_none());
        app.new_note();
        assert_eq!(app.notebook.notes.len(), 2);
        app.set_deleted(true);
        assert!(!app.notebook.notes.iter().any(|note| note.is_deleted));
        app.toggle_trash(true);
        assert!(!app.show_trash);
        assert!(!app.can_close());
        app.confirm_exit = true;
        let ctx = egui::Context::default();
        let output = run_headless(&ctx, Default::default(), |ctx| app.exit_dialog(ctx));
        assert!(!output.shapes.is_empty());
        app.discard_on_exit = true;
        assert!(app.can_close());
        assert_eq!(app.content, "不得丢失的修改");
        let rescued = app.snapshot_with_draft();
        assert_eq!(
            rescued
                .notes
                .iter()
                .find(|note| note.id == second_id)
                .unwrap()
                .content,
            "不得丢失的修改"
        );
    }

    #[test]
    fn autosave_writes_latest_buffer_and_allows_empty_existing_notes() {
        let (_directory, mut app) = app();
        app.new_note();
        app.title = "旧标题".into();
        app.content = "旧正文".into();
        app.edited();
        assert!(app.save_current());
        app.title.clear();
        app.content.clear();
        app.edited();
        app.last_edit = Some(Instant::now() - AUTOSAVE_DELAY);
        app.autosave(&egui::Context::default());
        assert!(!app.dirty);
        assert!(app.notebook.notes[0].title.is_empty());
        assert!(app.notebook.notes[0].content.is_empty());
        assert!(app.can_close());
    }

    #[test]
    fn hidden_window_logic_saves_draft_without_drawing_or_consuming_pending_input() {
        let (_directory, mut app) = app();
        app.new_note();
        app.content = "draft awaiting autosave".into();
        app.edited();
        app.last_edit = Some(Instant::now() - AUTOSAVE_DELAY);
        app.pending_input
            .push(egui::Event::Text("queued input".into()));
        eframe::App::logic(
            &mut app,
            &egui::Context::default(),
            &mut eframe::Frame::_new_kittest(),
        );
        assert!(!app.dirty);
        assert_eq!(app.notebook.notes[0].content, "draft awaiting autosave");
        assert_eq!(
            app.pending_input,
            vec![egui::Event::Text("queued input".into())]
        );
    }

    #[test]
    fn trash_and_restore_preserve_note_content_and_identity() {
        let (_directory, mut app) = app();
        app.new_note();
        let id = app.selected_id.clone().unwrap();
        app.content = "恢复后仍在".into();
        app.edited();
        app.set_deleted(true);
        assert!(app.selected_id.is_none());
        assert!(app.notebook.notes[0].is_deleted);
        app.toggle_trash(true);
        assert_eq!(app.selected_id.as_ref(), Some(&id));
        assert_eq!(app.content, "恢复后仍在");
        app.set_deleted(false);
        app.toggle_trash(false);
        assert_eq!(app.selected_id.as_ref(), Some(&id));
        assert_eq!(app.content, "恢复后仍在");
    }

    #[test]
    fn full_text_search_includes_body_and_pinned_notes_sort_first() {
        let (_directory, mut app) = app();
        app.new_note();
        let first = app.selected_id.clone().unwrap();
        app.content = "body needle".into();
        app.edited();
        app.toggle_pin();
        app.new_note();
        assert_eq!(app.visible_ids()[0], first);
        app.query = "NEEDLE".into();
        assert_eq!(app.visible_ids(), vec![first]);
    }

    #[test]
    fn imported_timestamps_are_sorted_by_instant_across_offsets() {
        let (_directory, mut app) = app();
        let mut earlier = Note::new("Earlier".into(), String::new());
        earlier.updated_at = "2026-10-08T10:30:00+08:00".into();
        let earlier_id = earlier.id.clone();
        let mut later = Note::new("Later".into(), String::new());
        later.updated_at = "2026-10-08T03:00:00Z".into();
        let later_id = later.id.clone();
        app.notebook.notes = vec![earlier, later];
        assert_eq!(app.visible_ids(), vec![later_id, earlier_id]);
    }

    #[test]
    fn failing_to_open_existing_data_never_enables_an_empty_replacement() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("notes.json");
        std::fs::write(&path, b"unreadable notebook").unwrap();
        let mut app = NotepadApp::from_storage(Storage::open(path.clone()));
        assert!(app.storage.is_none());
        app.new_note();
        assert!(app.notebook.notes.is_empty());
        assert!(app.error.is_some());
        assert!(app.can_close());
        assert_eq!(std::fs::read(path).unwrap(), b"unreadable notebook");
        let ctx = egui::Context::default();
        let _ = run_headless(&ctx, Default::default(), |ctx| app.render(ctx));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn manual_paths_import_export_and_rescue_unsaved_edits_without_a_portal() {
        let (directory, mut app) = app();
        let import = directory.path().join("中文.txt");
        std::fs::write(&import, "initial text").unwrap();
        assert!(app.submit_path(PathAction::Import, import.to_str().unwrap()));
        assert_eq!(app.content, "initial text");
        app.content = "unsaved rescue 中文".into();
        app.edited();
        fail_future_saves(&app);
        assert!(!app.can_close());
        let backup = directory.path().join("rescue.json");
        assert!(app.submit_path(PathAction::Backup, backup.to_str().unwrap()));
        let rescued: Notebook = serde_json::from_slice(&std::fs::read(&backup).unwrap()).unwrap();
        assert_eq!(rescued.notes[0].content, "unsaved rescue 中文");
        assert!(app.dirty);
        assert!(!app.can_close());
        let note = directory.path().join("rescue.txt");
        assert!(app.submit_path(PathAction::Note(false), note.to_str().unwrap()));
        assert_eq!(
            std::fs::read_to_string(note).unwrap(),
            "unsaved rescue 中文"
        );
        assert!(!app.submit_path(PathAction::Backup, backup.to_str().unwrap()));
        assert!(app.error.as_ref().unwrap().contains("already exists"));
        assert_eq!(
            serde_json::from_slice::<Notebook>(&std::fs::read(backup).unwrap()).unwrap(),
            rescued
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancelled_or_unavailable_picker_does_not_mutate_data_or_discard_errors() {
        let (_directory, mut app) = app();
        app.new_note();
        app.content = "draft".into();
        app.edited();
        app.error = Some("prior save error".into());
        let notebook = app.notebook.clone();
        app.file_dialog_dismissed();
        assert_eq!(app.notebook, notebook);
        assert_eq!(app.content, "draft");
        assert!(app.dirty);
        assert_eq!(app.error.as_deref(), Some("prior save error"));
        assert!(app.notice.as_ref().unwrap().contains("未选择文件"));
        assert!(!app.submit_path(PathAction::Backup, "~/backup.json"));
        assert!(app.error.as_ref().unwrap().contains("完整文件路径"));
        app.open_path_dialog(PathAction::Backup);
        let ctx = egui::Context::default();
        let output = run_headless(&ctx, Default::default(), |ctx| app.manual_path_dialog(ctx));
        assert!(!output.shapes.is_empty());
        assert!(app.path_dialog.is_some());
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = run_headless(&ctx, raw, |ctx| app.manual_path_dialog(ctx));
        assert!(app.path_dialog.is_none());
        assert!(app.dirty);
        assert_eq!(app.content, "draft");
    }

    #[test]
    fn system_cjk_font_renders_chinese_labels_when_available() {
        let ctx = egui::Context::default();
        if !install_system_font(&ctx) {
            assert_ne!(
                crate::runtime::option("REQUIRE_CJK_FONT").as_deref(),
                Some(std::ffi::OsStr::new("1")),
                "A system CJK font is required for this validation run"
            );
            eprintln!("SKIPPED glyph assertion: no supported system CJK font is installed");
            return;
        }
        let _ =
            run_headless(&ctx, Default::default(), |ctx| {
                assert!(ctx.fonts_mut(|fonts| fonts
                    .has_glyphs(&egui::FontId::proportional(16.0), "中文记事本保存回收站")));
                assert!(ctx.fonts_mut(|fonts| fonts
                    .has_glyphs(&egui::FontId::monospace(16.0), "中文记事本保存回收站")));
            });
        eprintln!("CJK glyph check passed for proportional and monospace text");
    }

    #[test]
    fn headless_egui_renders_empty_editing_trash_and_error_states() {
        let (_directory, mut app) = app();
        let ctx = egui::Context::default();
        let _ = run_headless(&ctx, Default::default(), |ctx| app.render(ctx));
        app.new_note();
        app.content = "测试 <script>alert(1)</script> 只是纯文本".into();
        app.edited();
        let output = run_headless(&ctx, Default::default(), |ctx| app.render(ctx));
        assert!(!output.shapes.is_empty());
        app.set_deleted(true);
        app.toggle_trash(true);
        let _ = run_headless(&ctx, Default::default(), |ctx| app.render(ctx));
        app.error = Some("磁盘已满".into());
        let _ = run_headless(&ctx, Default::default(), |ctx| app.render(ctx));
    }
}
