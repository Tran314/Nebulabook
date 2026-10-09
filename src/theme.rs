//! Lightweight, app-internal frosted surfaces. No desktop capture, transparency
//! API, blur pass, animation timer, network access, or platform dependency.
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub(crate) const BACKDROP_SIDE: usize = 160;

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub text: Color32,
    pub secondary: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub glass: Color32,
    pub paper: Color32,
    pub edge: Color32,
    pub line: Color32,
    pub success: Color32,
}

impl Palette {
    pub fn for_theme(dark: bool) -> Self {
        if dark {
            Self {
                text: Color32::from_rgb(233, 238, 246),
                secondary: Color32::from_rgb(167, 180, 200),
                accent: Color32::from_rgb(132, 182, 255),
                accent_soft: Color32::from_rgb(43, 65, 94),
                glass: Color32::from_rgba_unmultiplied(28, 38, 53, 200),
                paper: Color32::from_rgba_unmultiplied(28, 35, 47, 247),
                edge: Color32::from_rgba_unmultiplied(176, 196, 227, 45),
                line: Color32::from_rgba_unmultiplied(176, 196, 227, 35),
                success: Color32::from_rgb(132, 199, 171),
            }
        } else {
            Self {
                text: Color32::from_rgb(38, 48, 65),
                secondary: Color32::from_rgb(92, 107, 128),
                accent: Color32::from_rgb(43, 100, 173),
                accent_soft: Color32::from_rgb(220, 234, 249),
                glass: Color32::from_rgba_unmultiplied(247, 251, 255, 172),
                paper: Color32::from_rgba_unmultiplied(255, 255, 255, 246),
                edge: Color32::from_rgba_unmultiplied(255, 255, 255, 220),
                line: Color32::from_rgba_unmultiplied(66, 92, 125, 26),
                success: Color32::from_rgb(49, 112, 88),
            }
        }
    }
}

pub(crate) fn install(ctx: &egui::Context) {
    ctx.options_mut(|options| options.fallback_theme = egui::Theme::Light);
    ctx.set_theme(egui::ThemePreference::System);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        let p = Palette::for_theme(dark);
        let mut style = egui::Style::default();
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(26.0));
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size = egui::vec2(32.0, 32.0);
        style.spacing.menu_margin = egui::Margin::same(10);
        style.spacing.window_margin = egui::Margin::same(20);
        style.spacing.extra_text_line_spacing = 4.0;
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.override_text_color = Some(p.text);
        v.weak_text_color = Some(p.secondary);
        v.panel_fill = Color32::TRANSPARENT;
        // Menus/dialogs must not show underlying glyphs through their text.
        v.window_fill = Color32::from_rgb(p.paper.r(), p.paper.g(), p.paper.b());
        v.window_stroke = Stroke::new(1.0, p.edge);
        v.window_corner_radius = CornerRadius::same(16);
        v.menu_corner_radius = CornerRadius::same(12);
        v.window_shadow = egui::Shadow {
            offset: [0, 10],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 65 } else { 22 }),
        };
        v.popup_shadow = v.window_shadow;
        v.text_edit_bg_color = Some(if dark {
            Color32::from_rgb(37, 48, 65)
        } else {
            Color32::from_rgb(249, 251, 255)
        });
        v.extreme_bg_color = p.paper;
        v.faint_bg_color = p.glass;
        v.selection.bg_fill = p.accent_soft;
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.hyperlink_color = p.accent;
        v.warn_fg_color = if dark {
            Color32::from_rgb(243, 193, 121)
        } else {
            Color32::from_rgb(147, 91, 25)
        };
        v.error_fg_color = if dark {
            Color32::from_rgb(255, 161, 164)
        } else {
            Color32::from_rgb(176, 53, 66)
        };
        for widget in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(9);
            widget.fg_stroke = Stroke::new(1.0, p.text);
            widget.bg_stroke = Stroke::new(1.0, p.line);
            widget.expansion = 0.0;
        }
        v.widgets.inactive.weak_bg_fill = p.glass;
        v.widgets.hovered.weak_bg_fill = p.accent_soft;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.45));
        v.widgets.active.weak_bg_fill = p.accent_soft;
        v.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);
        v.widgets.open.weak_bg_fill = p.accent_soft;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
        style.visuals = v;
        ctx.set_style_of(theme, style);
    }
}

/// Gaussian color fields are sampled only once per theme, then linearly scaled
/// by the GPU. This is an app-internal diffuse background, not OS desktop blur.
pub(crate) fn backdrop_image(dark: bool) -> egui::ColorImage {
    let base = if dark {
        [24.0, 34.0, 51.0]
    } else {
        [232.0, 240.0, 249.0]
    };
    let fields = if dark {
        [
            (0.02, 0.06, [14.0, 40.0, 48.0]),
            (0.9, 0.15, [25.0, 17.0, 36.0]),
            (0.25, 0.94, [12.0, 28.0, 30.0]),
        ]
    } else {
        [
            (0.02, 0.06, [-31.0, -1.0, 1.0]),
            (0.9, 0.15, [3.0, -13.0, -1.0]),
            (0.25, 0.94, [-25.0, 1.0, -4.0]),
        ]
    };
    let mut pixels = Vec::with_capacity(BACKDROP_SIDE * BACKDROP_SIDE);
    for y in 0..BACKDROP_SIDE {
        for x in 0..BACKDROP_SIDE {
            let mut rgb = base;
            for (cx, cy, color) in fields {
                let dx = x as f32 / BACKDROP_SIDE as f32 - cx;
                let dy = y as f32 / BACKDROP_SIDE as f32 - cy;
                let weight = (-4.2 * (dx * dx + dy * dy)).exp();
                for c in 0..3 {
                    rgb[c] += color[c] * weight;
                }
            }
            // Static, sub-pixel grain prevents broad-gradient banding.
            let grain = ((x * 13 + y * 37 + x * y * 3) % 7) as f32 / 7.0 - 0.5;
            pixels.push(Color32::from_rgb(
                (rgb[0] + grain) as u8,
                (rgb[1] + grain) as u8,
                (rgb[2] + grain) as u8,
            ));
        }
    }
    egui::ColorImage::new([BACKDROP_SIDE, BACKDROP_SIDE], pixels)
}

pub(crate) fn note_preview(content: &str) -> String {
    let line = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("还没有正文")
        .trim();
    let mut preview: String = line.chars().take(88).collect();
    if line.chars().nth(88).is_some() {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backdrop_is_small_opaque_static_and_theme_specific() {
        let light = backdrop_image(false);
        assert_eq!(light.size, [160, 160]);
        assert_eq!(light.pixels.len() * 4, 102_400);
        assert!(light.pixels.iter().all(|pixel| pixel.a() == 255));
        assert_eq!(light, backdrop_image(false));
        assert_ne!(light, backdrop_image(true));
    }

    #[test]
    fn preview_is_bounded_and_handles_unicode_without_slicing_bytes() {
        assert_eq!(note_preview("\n  第二行  \n第三行"), "第二行");
        assert_eq!(note_preview("\n \n"), "还没有正文");
        let preview = note_preview(&"中文".repeat(100));
        assert_eq!(preview.chars().count(), 89);
        assert!(preview.ends_with('…'));
    }
}
