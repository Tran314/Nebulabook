//! Discover installed fonts only. No downloads or bundled proprietary fonts.
use eframe::egui;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn install_system_font(ctx: &egui::Context) -> bool {
    let mut candidates = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(font) = fontconfig_match() {
        candidates.push(font);
    }
    if let Some(windows) = std::env::var_os("WINDIR") {
        let fonts = PathBuf::from(windows).join("Fonts");
        candidates.extend([
            (fonts.join("msyh.ttc"), 0),
            (fonts.join("simhei.ttf"), 0),
            (fonts.join("simsun.ttc"), 0),
        ]);
    }
    // Fallbacks still work on minimal desktops without fontconfig installed.
    candidates.extend(
        [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        ]
        .map(|path| (PathBuf::from(path), 0)),
    );
    for (path, index) in candidates {
        let Some(bytes) = read_font(&path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        fonts.font_data.insert("system-cjk".into(), data.into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("system-cjk".into());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

fn read_font(path: &Path) -> Option<Vec<u8>> {
    // Check before opening too: a FIFO masquerading as a font can block in open.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FONT_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FONT_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_FONT_BYTES).then_some(bytes)
}

#[cfg(target_os = "linux")]
fn fontconfig_match() -> Option<(PathBuf, u32)> {
    let output = bounded_output(
        std::process::Command::new("fc-match").args([
            "--format=%{file}\n%{index}\n%{lang}\n",
            "sans-serif:lang=zh-cn",
        ]),
        std::time::Duration::from_millis(1500),
    )?;
    parse_fontconfig_match(&output)
}

#[cfg(target_os = "linux")]
fn parse_fontconfig_match(output: &[u8]) -> Option<(PathBuf, u32)> {
    use std::os::unix::ffi::OsStrExt;
    let mut lines = output.split(|byte| *byte == b'\n');
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(lines.next()?));
    let index = std::str::from_utf8(lines.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let languages = std::str::from_utf8(lines.next()?).ok()?;
    // Fontconfig returns a non-CJK default even when no Chinese font exists.
    // Ignore unsupported webfonts and variable-font named instance indices.
    if !path.is_absolute()
        || index > u16::MAX as u32
        || !languages
            .split('|')
            .any(|language| matches!(language, "zh-cn" | "zh-sg" | "zh"))
        || !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("ttf" | "otf" | "ttc" | "otc")
        )
    {
        return None;
    }
    Some((path, index))
}

#[cfg(target_os = "linux")]
fn bounded_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Option<Vec<u8>> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    const MAX_OUTPUT: u64 = 16 * 1024;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("fontconfig-output".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .by_ref()
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .ok()
                .filter(|_| bytes.len() as u64 <= MAX_OUTPUT)
                .map(|_| bytes);
            let _ = sender.send(result);
        })
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return status
                    .success()
                    .then(|| {
                        receiver
                            .recv_timeout(Duration::from_millis(50))
                            .ok()
                            .flatten()
                    })
                    .flatten()
            }
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_oversized_or_non_file_font_candidates() {
        let directory = tempfile::tempdir().unwrap();
        assert!(read_font(directory.path()).is_none());
        let path = directory.path().join("large.ttf");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_FONT_BYTES + 1)
            .unwrap();
        assert!(read_font(&path).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fontconfig_handles_custom_paths_and_collection_faces() {
        let output = "/home/me/.local/share/fonts/中文 font.ttc\n2\nen|zh-cn|zh-tw\n";
        assert_eq!(
            parse_fontconfig_match(output.as_bytes()),
            Some((
                PathBuf::from("/home/me/.local/share/fonts/中文 font.ttc"),
                2
            ))
        );
        for invalid in [
            "/fonts/latin.ttf\n0\nen|fr\n",
            "/fonts/chinese.woff2\n0\nzh-cn\n",
            "relative.ttf\n0\nzh-cn\n",
            "/fonts/chinese.ttf\ninvalid\nzh-cn\n",
            "/fonts/chinese.ttf\n65536\nzh-cn\n",
        ] {
            assert!(parse_fontconfig_match(invalid.as_bytes()).is_none());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn font_discovery_has_a_timeout_and_rejects_failed_commands() {
        use std::process::Command;
        use std::time::{Duration, Instant};
        let start = Instant::now();
        assert!(
            bounded_output(Command::new("sleep").arg("5"), Duration::from_millis(40)).is_none()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(bounded_output(
            Command::new("sh").args(["-c", "exit 1"]),
            Duration::from_secs(1)
        )
        .is_none());
        assert_eq!(
            bounded_output(
                Command::new("printf").arg("font output"),
                Duration::from_secs(1)
            ),
            Some(b"font output".to_vec())
        );
    }
}
