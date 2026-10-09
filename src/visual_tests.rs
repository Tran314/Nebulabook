//! Opt-in Linux visual evidence from the actual egui app and Glow renderer.
//! Uses Mesa's EGL surfaceless platform, never a window, display server, or
//! desktop capture. This is layout/rendering QA, not native-window acceptance.

use super::*;
use eframe::{egui_glow, glow};
use glow::HasContext;
use std::ffi::{c_char, c_void, CString};
use std::io::Write;
use std::ptr;
use std::sync::Arc;

type EglHandle = *mut c_void;

// Only this ignored Linux test links EGL. The application has no added runtime
// dependency, and neither this module nor this linkage exists in normal builds.
#[link(name = "libEGL.so.1", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn eglGetPlatformDisplay(platform: u32, native: EglHandle, attrs: *const isize) -> EglHandle;
    fn eglInitialize(display: EglHandle, major: *mut i32, minor: *mut i32) -> u32;
    fn eglBindAPI(api: u32) -> u32;
    fn eglChooseConfig(
        display: EglHandle,
        attrs: *const i32,
        config: *mut EglHandle,
        size: i32,
        count: *mut i32,
    ) -> u32;
    fn eglCreatePbufferSurface(
        display: EglHandle,
        config: EglHandle,
        attrs: *const i32,
    ) -> EglHandle;
    fn eglCreateContext(
        display: EglHandle,
        config: EglHandle,
        share: EglHandle,
        attrs: *const i32,
    ) -> EglHandle;
    fn eglMakeCurrent(
        display: EglHandle,
        draw: EglHandle,
        read: EglHandle,
        context: EglHandle,
    ) -> u32;
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
    fn eglDestroyContext(display: EglHandle, context: EglHandle) -> u32;
    fn eglDestroySurface(display: EglHandle, surface: EglHandle) -> u32;
    fn eglTerminate(display: EglHandle) -> u32;
    fn eglGetError() -> u32;
}

struct OffscreenContext {
    display: EglHandle,
    surface: EglHandle,
    context: EglHandle,
}

impl OffscreenContext {
    fn new(width: u32, height: u32) -> Self {
        // SAFETY: EGL uses opaque handles. Every attribute array is terminated
        // with EGL_NONE, and all output pointers refer to live local variables.
        // The returned context stays current on this single test thread until
        // its painter is destroyed and this guard drops.
        unsafe {
            const EGL_NONE: i32 = 0x3038;
            const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
            const EGL_OPENGL_API: u32 = 0x30A2;
            let display =
                eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null());
            assert!(!display.is_null(), "No Mesa EGL surfaceless display");
            let (mut major, mut minor) = (0, 0);
            assert_ne!(eglInitialize(display, &mut major, &mut minor), 0);
            assert_ne!(eglBindAPI(EGL_OPENGL_API), 0);
            let attrs = [
                0x3033, 0x0001, // EGL_SURFACE_TYPE, EGL_PBUFFER_BIT
                0x3040, 0x0008, // EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT
                0x3024, 8, // EGL_RED_SIZE
                0x3023, 8, // EGL_GREEN_SIZE
                0x3022, 8, // EGL_BLUE_SIZE
                0x3021, 8, // EGL_ALPHA_SIZE
                EGL_NONE,
            ];
            let mut config = ptr::null_mut();
            let mut count = 0;
            assert_ne!(
                eglChooseConfig(display, attrs.as_ptr(), &mut config, 1, &mut count),
                0
            );
            assert_eq!(count, 1, "No RGBA8 OpenGL pbuffer configuration");
            let attrs = [0x3057, width as i32, 0x3056, height as i32, EGL_NONE];
            let surface = eglCreatePbufferSurface(display, config, attrs.as_ptr());
            assert!(
                !surface.is_null(),
                "EGL pbuffer error: {:#x}",
                eglGetError()
            );
            let context = eglCreateContext(display, config, ptr::null_mut(), [EGL_NONE].as_ptr());
            assert!(
                !context.is_null(),
                "EGL context error: {:#x}",
                eglGetError()
            );
            assert_ne!(eglMakeCurrent(display, surface, surface, context), 0);
            Self {
                display,
                surface,
                context,
            }
        }
    }

    fn glow(&self) -> Arc<glow::Context> {
        // SAFETY: this guard owns the current GL context; the caller keeps it
        // alive until all Glow resources have been explicitly destroyed.
        Arc::new(unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).expect("GL function name contains NUL");
                eglGetProcAddress(name.as_ptr())
            })
        })
    }
}

impl Drop for OffscreenContext {
    fn drop(&mut self) {
        // SAFETY: all handles belong to this guard, and the painter is destroyed
        // before the guard drops. No EGL or Glow calls follow this cleanup.
        unsafe {
            eglMakeCurrent(
                self.display,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            eglDestroyContext(self.display, self.context);
            eglDestroySurface(self.display, self.surface);
            eglTerminate(self.display);
        }
    }
}

fn visual_fixture(app: &mut NotepadApp) {
    let entries = [
        (
            "留一点空间，给新的想法",
            "把日常里一闪而过的灵感，慢慢写下来。\n\n今天想完成的三件小事\n01  整理桌面，让注意力回到眼前\n02  记录一个值得继续探索的想法\n03  留出一段不被打扰的阅读时间\n\n关于这个工作空间\n柔和的光、清楚的层次，以及恰到好处的留白。工具安静一些，思考就可以自由一些。\n\n中文与 English 可以自然地放在同一段文字里。长句需要在窄窗口中正常换行，文字不应被侧边栏或操作按钮遮住。",
        ),
        ("本周计划 · 保持专注", "周一：整理阅读笔记\n周三：复盘项目\n周五：为下周留出空间"),
        ("界面灵感 · Glass Study", "半透明的层次、柔和的边缘、清晰的文字。\n只保留真正有用的动作。"),
        ("读书摘录", "记录值得回看的句子，也记下当时自己的问题。"),
        ("周末散步路线", "沿着河岸走一走。\n带上相机，看看傍晚的光落在哪里。"),
        ("一个很长的标题，用来检查笔记列表是否能优雅地截断而不挤压其他内容", "长标题和短预览都应该保持可读。"),
        ("随手记", "咖啡、清单、突然出现的小想法。\n无需整理完美，先记录下来。"),
        ("暂存的旧方案", "这是用于回收站截图的演示内容。\n删除后仍保留正文，需要时可以恢复。"),
    ];
    app.notebook.notes = entries
        .iter()
        .enumerate()
        .map(|(index, (title, content))| {
            let mut note = Note::new((*title).into(), (*content).into());
            note.id = format!("visual-test-{index}");
            note.created_at = "2026-10-01T08:00:00+00:00".into();
            note.updated_at = format!("2026-10-08T09:{:02}:00+00:00", 20 - index);
            note.is_pinned = index < 2;
            note.is_deleted = index == entries.len() - 1;
            note.deleted_at = note.is_deleted.then(|| note.updated_at.clone());
            note
        })
        .collect();
    app.select_first();
}

fn write_ppm(path: &Path, image: &egui::ColorImage) {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
    for pixel in &image.pixels {
        file.write_all(&[pixel.r(), pixel.g(), pixel.b()]).unwrap();
    }
    file.flush().unwrap();
}

#[test]
#[ignore = "Opt-in actual Glow screenshots; requires Mesa EGL surfaceless and a CJK font"]
fn capture_native_egui_visuals() {
    let output = std::env::var_os("NEBULABOOK_VISUAL_OUTPUT")
        .map(std::path::PathBuf::from)
        .expect("Set NEBULABOOK_VISUAL_OUTPUT to the visual evidence directory");
    std::fs::create_dir_all(&output).unwrap();
    let cases = [
        ("light-editor", [1000.0, 700.0], 1.0, false),
        ("dark-editor", [1000.0, 700.0], 1.0, true),
        ("light-compact", [640.0, 420.0], 1.0, false),
        ("dark-compact", [640.0, 420.0], 1.0, true),
        ("light-2x", [1000.0, 700.0], 2.0, false),
        ("empty", [1000.0, 700.0], 1.0, false),
        ("search-none", [1000.0, 700.0], 1.0, false),
        ("trash", [1000.0, 700.0], 1.0, false),
        ("trash-dark", [1000.0, 700.0], 1.0, true),
        ("trash-long-scrolled", [640.0, 420.0], 1.0, false),
        ("save-error", [640.0, 420.0], 1.0, false),
        ("exit-error", [640.0, 420.0], 1.0, false),
        ("manual-path", [640.0, 420.0], 1.0, false),
        ("export-menu", [640.0, 420.0], 1.0, false),
        ("export-menu-dark", [640.0, 420.0], 1.0, true),
        ("long-title", [640.0, 420.0], 1.0, false),
    ];
    for (name, logical_size, scale, dark) in cases {
        let size = [
            (logical_size[0] * scale) as u32,
            (logical_size[1] * scale) as u32,
        ];
        let egl = OffscreenContext::new(size[0], size[1]);
        let gl = egl.glow();
        // SAFETY: EGL made this GL context current on this thread above.
        let renderer = unsafe { gl.get_parameter_string(glow::RENDERER) };
        let mut painter = egui_glow::Painter::new(gl.clone(), "", None, true).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        assert!(
            install_system_font(&ctx),
            "Visual evidence requires CJK font"
        );
        let directory = tempfile::tempdir().unwrap();
        let mut app =
            NotepadApp::from_storage(Storage::open(directory.path().join("notes.nebula")));
        assert!(app.storage.is_some());
        app.check_cjk_font = true;
        if name != "empty" {
            visual_fixture(&mut app);
        }
        match name {
            "search-none" => app.query = "no-note-matches-7f2b".into(),
            "trash" | "trash-dark" | "trash-long-scrolled" => {
                if name == "trash-long-scrolled" {
                    let note = app
                        .notebook
                        .notes
                        .iter_mut()
                        .find(|note| note.is_deleted)
                        .unwrap();
                    note.content = (1..=60)
                        .map(|line| format!("滚动验收 {line:02}：回收站长笔记保持只读。\n"))
                        .collect();
                }
                app.show_trash = true;
                app.select_first();
            }
            "save-error" | "exit-error" => {
                app.dirty = true;
                app.error =
                    Some("保存失败：磁盘空间不足。修改仍在编辑器中，请重试或导出备份。".into());
                app.confirm_exit = name == "exit-error";
            }
            "manual-path" => {
                app.open_path_dialog(PathAction::Backup);
            }
            "long-title" => {
                app.query = "一个很长".into();
                app.select_first();
            }
            _ => {}
        }
        let mut events = Vec::new();
        let mut original_text_y = None;
        let mut scrolled_text_y = None;
        // Include enough simulated frame time for egui popup/window fades to
        // settle; a half-open animation is not the final visual state.
        for frame in 0..20 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(logical_size[0], logical_size[1]),
                )),
                system_theme: Some(if dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                }),
                time: Some(frame as f64 / 60.0),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let mut result = ctx.run_ui(input, |ui| app.frame(ui));
            if frame == 19 && matches!(name, "light-editor" | "light-compact") {
                let trash = result
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            if text.galley.job.text == "回收站" {
                                return Some(text.pos + text.galley.size() * 0.5);
                            }
                        }
                        None
                    })
                    .expect("Trash toolbar button was not rendered");
                assert!(
                    trash.y < 70.0 && trash.x < logical_size[0],
                    "Trash entry must remain in the visible top toolbar"
                );
                eprintln!(
                    "Client-area click target: {name} 回收站 x={:.1}, y={:.1} logical pixels",
                    trash.x, trash.y
                );
            }
            if name == "trash-long-scrolled" {
                let position = result
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            if text.galley.job.text == app.content {
                                return Some(text.pos);
                            }
                        }
                        None
                    })
                    .expect("Long read-only editor was not rendered");
                if frame == 1 {
                    original_text_y = Some(position.y);
                    events = vec![
                        egui::Event::PointerMoved(position + egui::vec2(10.0, 10.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -420.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Default::default(),
                        },
                    ];
                }
                scrolled_text_y = Some(position.y);
            }
            if frame == 1 && matches!(name, "export-menu" | "export-menu-dark") {
                let position = result
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            if text.galley.job.text == "导入 / 导出" {
                                return Some(text.pos + text.galley.size() * 0.5);
                            }
                        }
                        None
                    })
                    .expect("Export menu button was not rendered");
                events = vec![
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
                ];
            }
            let primitives = ctx.tessellate(result.shapes, result.pixels_per_point);
            painter.clear(size, [0.95, 0.95, 0.97, 1.0]);
            painter.paint_and_update_textures(
                size,
                result.pixels_per_point,
                &primitives,
                &mut result.textures_delta,
            );
            assert!((result.pixels_per_point - scale).abs() < 0.01);
        }
        if name == "trash-long-scrolled" {
            assert!(
                scrolled_text_y.unwrap() < original_text_y.unwrap() - 100.0,
                "Read-only note must respond to its scroll wheel"
            );
            assert!(!app.dirty, "Scrolling must not edit the deleted note");
        }
        assert!(app.has_cjk_font);
        let image = painter.read_screen_rgba(size);
        assert!(image.pixels.windows(2).any(|pixels| pixels[0] != pixels[1]));
        write_ppm(&output.join(format!("{name}.ppm")), &image);
        eprintln!(
            "Visual evidence: {name} {}×{}, {scale}×, {renderer}",
            size[0], size[1]
        );
        painter.destroy();
    }
}
