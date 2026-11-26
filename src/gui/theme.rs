//! Colours, fonts and egui style, after the swarm2linux "Swarm" theme.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const BACKGROUND: Color32 = c(0x111419);
pub const RAIL: Color32 = c(0x0e1014);
pub const RAIL_HOVER: Color32 = c(0x15181e);
pub const RAIL_ACTIVE: Color32 = c(0x181c23);
pub const LINE: Color32 = c(0x232831);
pub const CARD_TOP: Color32 = c(0x1d222b);
pub const CARD_BOTTOM: Color32 = c(0x191d25);
pub const INSET: Color32 = c(0x1f242c);
pub const FIELD: Color32 = c(0x252b34);
pub const HOVER: Color32 = c(0x262d37);
pub const POPUP: Color32 = c(0x20262f);
pub const SEGMENT: Color32 = c(0x313844);
pub const OUTLINE: Color32 = c(0x4a515c);
pub const TEXT: Color32 = c(0xe9ebee);
pub const MUTED: Color32 = c(0x8d949e);
pub const DIM: Color32 = c(0x5b626c);
pub const ACCENT: Color32 = c(0x7b45f5);
pub const FILL_START: Color32 = c(0x6b3ef0);
pub const FILL_END: Color32 = c(0xe2459c);
pub const TRACK: Color32 = c(0x3a3f48);
pub const KNOB: Color32 = c(0xffffff);
pub const TOGGLE_OFF: Color32 = c(0x7d838c);
pub const SELECTED: Color32 = c(0x3dd6b8);
pub const SELECTED_SOFT: Color32 = c(0x173a35);
pub const GOOD: Color32 = c(0x35c47f);
pub const GOOD_SOFT: Color32 = c(0x16302a);
pub const WARN: Color32 = c(0xe3a23f);
pub const WARN_SOFT: Color32 = c(0x2a2416);
pub const BAD: Color32 = c(0xe5484d);

pub const TITLE_SIZE: f32 = 17.0;
pub const LABEL_SIZE: f32 = 14.0;
pub const SMALL_SIZE: f32 = 12.0;
pub const RADIUS: u8 = 6;
pub const PAD: f32 = 32.0;

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn load_font(fonts: &mut FontDefinitions, key: &str, paths: &[&str]) -> bool {
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert(key.to_owned(), Arc::new(FontData::from_owned(bytes)));
            return true;
        }
    }
    false
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let regular = load_font(&mut fonts, "noto", &[
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    ]);
    let bold_ok = load_font(&mut fonts, "noto-bold", &[
        "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
    ]);
    if regular {
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "noto".into());
    }
    let mut b = fonts.families[&FontFamily::Proportional].clone();
    if bold_ok {
        b.insert(0, "noto-bold".into());
    }
    fonts.families.insert(bold(), b);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(SMALL_SIZE)),
        (TextStyle::Body, FontId::proportional(LABEL_SIZE)),
        (TextStyle::Button, FontId::proportional(LABEL_SIZE)),
        (TextStyle::Heading, FontId::new(TITLE_SIZE, bold())),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.combo_height = 320.0;

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BACKGROUND;
    v.window_fill = POPUP;
    v.window_stroke = egui::Stroke::new(1.0_f32, LINE);
    v.window_corner_radius = egui::CornerRadius::same(RADIUS);
    v.menu_corner_radius = egui::CornerRadius::same(RADIUS);
    v.extreme_bg_color = INSET;
    v.faint_bg_color = CARD_BOTTOM;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = SELECTED;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = egui::Stroke::new(1.0_f32, TEXT);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(120) };
    v.window_shadow = v.popup_shadow;
    let w = &mut v.widgets;
    for (state, fill) in [
        (&mut w.noninteractive, CARD_TOP),
        (&mut w.inactive, FIELD),
        (&mut w.hovered, HOVER),
        (&mut w.active, SEGMENT),
        (&mut w.open, HOVER),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = egui::CornerRadius::same(RADIUS);
        state.bg_stroke = egui::Stroke::NONE;
        state.expansion = 0.0;
    }
    w.noninteractive.fg_stroke.color = TEXT;
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, LINE);
    w.inactive.fg_stroke.color = TEXT;
    w.hovered.fg_stroke.color = TEXT;
    w.active.fg_stroke.color = TEXT;
    ctx.set_style(style);
}
