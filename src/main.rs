#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run_app() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_startup_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn run_app() -> eframe::Result<()> {
    eframe::run_native(
        "Nebulabook",
        native_options(),
        Box::new(|cc| {
            if !error_dialog_enabled(std::env::var_os("NEBULABOOK_NO_ERROR_DIALOG").as_deref()) {
                #[cfg(windows)]
                if let Some(render_state) = &cc.wgpu_render_state {
                    let adapter = render_state.adapter.get_info();
                    let _ = writeln!(
                        io::stderr().lock(),
                        "Nebulabook renderer initialized: backend={:?}, type={:?}, name={}",
                        adapter.backend,
                        adapter.device_type,
                        adapter.name
                    );
                }
                #[cfg(not(windows))]
                if let Some(gl) = &cc.gl {
                    use eframe::glow::HasContext;
                    let _ = writeln!(
                        io::stderr().lock(),
                        "Nebulabook renderer initialized: backend=Glow, OpenGL={:?}",
                        gl.version()
                    );
                }
            }
            Ok(Box::new(nebulabook::app::NotepadApp::new(cc)))
        }),
    )
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nebulabook · 轻量记事本")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([640.0, 420.0]),
        #[cfg(windows)]
        renderer: eframe::Renderer::Wgpu,
        #[cfg(windows)]
        wgpu_options: windows_wgpu_options(),
        #[cfg(not(windows))]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

#[cfg(windows)]
fn windows_wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::{egui_wgpu, wgpu};
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    // Use Windows 10+'s native graphics stack, rather than relying on a GPU
    // vendor's OpenGL driver. Default adapter selection tries hardware before
    // CPU adapters, including the OS-provided Microsoft WARP renderer.
    setup.instance_descriptor.backends = wgpu::Backends::DX12;
    setup.power_preference = wgpu::PowerPreference::HighPerformance;
    // FXC uses the system compiler; do not require/bundle extra DXC DLLs.
    setup
        .instance_descriptor
        .backend_options
        .dx12
        .shader_compiler = wgpu::Dx12Compiler::Fxc;
    egui_wgpu::WgpuConfiguration {
        wgpu_setup: setup.into(),
        ..Default::default()
    }
}

fn rendering_backend() -> &'static str {
    #[cfg(windows)]
    {
        "wgpu / Direct3D 12 (hardware preferred, WARP fallback)"
    }
    #[cfg(not(windows))]
    {
        "glow / OpenGL"
    }
}

fn startup_diagnostic(error: &eframe::Error) -> String {
    // Only engine/build diagnostics: never inspect the notebook, credentials,
    // user profile, or the full environment while reporting an engine failure.
    format!(
        "Nebulabook native startup/runtime failure\nVersion: {}\nPlatform: {} {}\nProcess: {}\nRendering backend: {}\nError: {error}\nDetails: {error:?}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::process::id(),
        rendering_backend(),
    )
}

fn write_startup_log(path: &Path, diagnostic: &str) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    // A mistaken diagnostics path must not overwrite a notebook or other file.
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(diagnostic.as_bytes())?;
    file.sync_all()
}

fn error_dialog_enabled(value: Option<&OsStr>) -> bool {
    value != Some(OsStr::new("1"))
}

fn report_startup_error(error: &eframe::Error) {
    let diagnostic = startup_diagnostic(error);
    // A Windows GUI executable may not have usable standard streams. The
    // explicitly selected log path also makes noninteractive CI failures visible.
    let _ = io::stderr().lock().write_all(diagnostic.as_bytes());
    let log_status = match std::env::var_os("NEBULABOOK_STARTUP_LOG") {
        Some(path) => match write_startup_log(Path::new(&path), &diagnostic) {
            Ok(()) => format!("\n诊断日志：{}", Path::new(&path).display()),
            Err(log_error) => {
                let message = format!("Unable to create startup diagnostic log: {log_error}\n");
                let _ = io::stderr().lock().write_all(message.as_bytes());
                format!("\n诊断日志写入失败：{log_error}")
            }
        },
        None => String::new(),
    };
    if error_dialog_enabled(std::env::var_os("NEBULABOOK_NO_ERROR_DIALOG").as_deref()) {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Nebulabook 无法启动")
            .set_description(format!(
                "无法初始化或运行图形窗口。请保留下面的诊断信息以便排查。\n\n{diagnostic}{log_status}"
            ))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_report_preserves_engine_error_and_build_context() {
        let error = eframe::Error::AppCreation(Box::new(io::Error::other("test engine failure")));
        let report = startup_diagnostic(&error);
        assert!(report.contains("test engine failure"));
        assert!(report.contains("AppCreation"));
        assert!(report.contains(rendering_backend()));
        assert!(report.contains(env!("CARGO_PKG_VERSION")));
        assert!(report.contains(std::env::consts::OS));
        assert!(report.contains(std::env::consts::ARCH));
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_dx12_with_default_hardware_and_warp_selection() {
        use eframe::{egui_wgpu, wgpu};
        let options = native_options();
        assert_eq!(options.renderer, eframe::Renderer::Wgpu);
        let egui_wgpu::WgpuSetup::CreateNew(setup) = options.wgpu_options.wgpu_setup else {
            panic!("Expected native adapter discovery");
        };
        assert_eq!(setup.instance_descriptor.backends, wgpu::Backends::DX12);
        // The direct dependency supplies DX12 features to eframe's exact wgpu major.
        // A split upgrade must fail at compile time rather than lose its backend.
        let direct_backend: ::wgpu::Backends = setup.instance_descriptor.backends;
        assert_eq!(direct_backend, ::wgpu::Backends::DX12);
        assert_eq!(
            setup.power_preference,
            wgpu::PowerPreference::HighPerformance
        );
        assert!(setup.native_adapter_selector.is_none());
        assert!(matches!(
            setup
                .instance_descriptor
                .backend_options
                .dx12
                .shader_compiler,
            wgpu::Dx12Compiler::Fxc
        ));
        assert!(rendering_backend().contains("Direct3D 12"));
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_keep_the_existing_glow_backend() {
        assert_eq!(native_options().renderer, eframe::Renderer::Glow);
        assert_eq!(rendering_backend(), "glow / OpenGL");
    }

    #[test]
    fn startup_log_is_readable_without_a_console() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup.log");
        write_startup_log(&path, "test diagnostic\n").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "test diagnostic\n");
    }

    #[cfg(unix)]
    #[test]
    fn startup_log_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup.log");
        write_startup_log(&path, "diagnostic").unwrap();
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn startup_log_never_overwrites_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notebook.nebula");
        std::fs::write(&path, "existing notebook").unwrap();
        let error = write_startup_log(&path, "diagnostic").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "existing notebook");
    }

    #[test]
    fn an_unwritable_diagnostic_path_is_reported_as_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing-parent").join("startup.log");
        assert!(write_startup_log(&path, "diagnostic").is_err());
    }

    #[test]
    fn only_the_explicit_noninteractive_flag_suppresses_the_dialog() {
        assert!(!error_dialog_enabled(Some(OsStr::new("1"))));
        assert!(error_dialog_enabled(None));
        assert!(error_dialog_enabled(Some(OsStr::new("0"))));
        assert!(error_dialog_enabled(Some(OsStr::new("true"))));
    }
}
