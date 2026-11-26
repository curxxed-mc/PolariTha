//! Custom widgets in the swarm2linux style.

use super::theme::*;
use eframe::egui::{
    self, emath::Numeric, pos2, text::LayoutJob, vec2, Align, Color32, CornerRadius, FontId, Layout, Mesh, Pos2, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, TextFormat, Ui,
};
use std::ops::RangeInclusive;

/// Letter-spaced text (the swarm look uses 1px spacing on all caps labels).
pub fn spaced(text: &str, size: f32, color: Color32, bold_font: bool, spacing: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = if bold_font { FontId::new(size, bold()) } else { FontId::proportional(size) };
    job.append(text, 0.0, TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    job
}

pub fn caps(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(egui::Label::new(spaced(&text.to_uppercase(), LABEL_SIZE, color, false, 1.0)).selectable(false))
}

pub fn small_caps(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(egui::Label::new(spaced(&text.to_uppercase(), SMALL_SIZE - 1.0, color, false, 1.0)).selectable(false))
}

pub fn hint(ui: &mut Ui, text: &str) {
    // built as a LayoutJob so justified column layouts don't stretch the words
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat { font_id: FontId::proportional(SMALL_SIZE), color: MUTED, ..Default::default() });
    job.wrap.max_width = ui.available_width();
    job.halign = Align::LEFT;
    job.justify = false;
    let galley = ui.fonts(|f| f.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y), Sense::hover());
    ui.painter().galley(rect.left_top(), galley, MUTED);
}

pub fn muted(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).color(MUTED)).selectable(false))
}

/// A dark rounded card with a spaced caps title. `fill_width` makes it span the column.
pub fn card<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(CARD_TOP).corner_radius(CornerRadius::same(RADIUS)).inner_margin(egui::Margin::same(24));
    let mut out = None;
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
        if !title.is_empty() {
            ui.add(egui::Label::new(spaced(&title.to_uppercase(), TITLE_SIZE, TEXT, false, 1.0)).selectable(false));
            ui.add_space(8.0);
        }
        out = Some(add(ui));
    });
    out.unwrap()
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

fn dim(c: Color32) -> Color32 {
    c.gamma_multiply(0.45)
}

/// Horizontal gradient bar.
fn gradient_bar(painter: &egui::Painter, rect: Rect, a: Color32, b: Color32) {
    let mut mesh = Mesh::default();
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.right_top(), b);
    mesh.colored_vertex(rect.right_bottom(), b);
    mesh.colored_vertex(rect.left_bottom(), a);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
    painter.add(Shape::mesh(mesh));
}

/// Gradient slider with a white knob; fills the available width.
pub fn slider<T: Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>, enabled: bool) -> Response {
    let width = ui.available_width().max(80.0);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 22.0), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    let knob_r = 8.0;
    let left = rect.left() + knob_r;
    let right = rect.right() - knob_r;
    if enabled {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - left) / (right - left)).clamp(0.0, 1.0) as f64;
            let mut v = lo + t * (hi - lo);
            if T::INTEGRAL {
                v = v.round();
            }
            let nv = T::from_f64(v.clamp(lo.min(hi), hi.max(lo)));
            if nv.to_f64() != value.to_f64() {
                *value = nv;
                resp.mark_changed();
            }
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    let t = if hi == lo { 0.0 } else { ((value.to_f64() - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 };
    let cy = rect.center().y;
    let x = left + t * (right - left);
    let painter = ui.painter_at(rect.expand(2.0));
    painter.line_segment([pos2(left, cy), pos2(right, cy)], Stroke::new(2.0_f32, if enabled { TRACK } else { dim(TRACK) }));
    let (a, b) = if enabled { (FILL_START, lerp_color(FILL_START, FILL_END, 1.0)) } else { (dim(FILL_START), dim(FILL_END)) };
    if x > left {
        gradient_bar(&painter, Rect::from_min_max(pos2(left, cy - 1.0), pos2(x, cy + 1.0)), a, b);
    }
    let knob = if enabled { KNOB } else { TOGGLE_OFF.gamma_multiply(0.7) };
    let r = if resp.dragged() || resp.hovered() { knob_r + 1.0 } else { knob_r };
    painter.circle_filled(pos2(x, cy), r, knob);
    resp
}

/// Caps label (+ optional value on the right) above a slider with min/max labels.
pub fn slider_row<T: Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    min_label: &str,
    max_label: &str,
    value_text: Option<String>,
    enabled: bool,
) -> bool {
    let color = if enabled { TEXT } else { DIM };
    if !label.is_empty() || value_text.is_some() {
        ui.horizontal(|ui| {
            caps(ui, label, color);
            if let Some(v) = &value_text {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(v).color(if enabled { MUTED } else { DIM }));
                });
            }
        });
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        let lc = if enabled { MUTED } else { DIM };
        ui.add_sized([28.0, 22.0], egui::Label::new(egui::RichText::new(min_label).color(lc)));
        let w = (ui.available_width() - 36.0 - ui.spacing().item_spacing.x).max(60.0);
        ui.allocate_ui(vec2(w, 22.0), |ui| {
            changed = slider(ui, value, range, enabled).changed();
        });
        ui.add_sized([36.0, 22.0], egui::Label::new(egui::RichText::new(max_label).color(lc)));
    });
    changed
}

/// Pill switch.
pub fn toggle(ui: &mut Ui, on: &mut bool, enabled: bool) -> Response {
    let size = vec2(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let mut bg = lerp_color(TRACK, ACCENT, t);
    let mut knob = lerp_color(TOGGLE_OFF, KNOB, t);
    if !enabled {
        bg = dim(bg);
        knob = dim(knob);
    }
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(11), bg);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 7.0, knob);
    resp
}

/// Caps label on the left, switch on the right.
pub fn toggle_row(ui: &mut Ui, label: &str, on: &mut bool, enabled: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        caps(ui, label, if enabled { TEXT } else { DIM });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = toggle(ui, on, enabled).changed();
        });
    });
    changed
}

/// Segmented control. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], enabled: bool, width: f32) -> bool {
    let mut changed = false;
    let seg_w = ((width - 6.0) / options.len() as f32).max(36.0);
    let total = vec2(seg_w * options.len() as f32 + 6.0, 36.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), FIELD);
    for (i, (v, label)) in options.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + 3.0 + i as f32 * seg_w, rect.top() + 3.0), vec2(seg_w, 30.0));
        let resp = ui.interact(r, ui.id().with(("seg", label, i)), if enabled { Sense::click() } else { Sense::hover() });
        let sel = *value == *v;
        if sel {
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS - 1), if enabled { SEGMENT } else { dim(SEGMENT) });
        } else if resp.hovered() && enabled {
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS - 1), HOVER);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let col = if !enabled { DIM } else if sel { TEXT } else { MUTED };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *label, FontId::proportional(LABEL_SIZE), col);
        if resp.clicked() && !sel {
            *value = v.clone();
            changed = true;
        }
    }
    changed
}

/// One entry of a dot radio list (selected = teal text and filled dot).
pub fn radio_item(ui: &mut Ui, selected: bool, text: &str, enabled: bool) -> Response {
    let job = spaced(&text.to_uppercase(), LABEL_SIZE, if !enabled { DIM } else if selected { SELECTED } else { TEXT }, false, 0.6);
    let galley = ui.fonts(|f| f.layout_job(job));
    let h = 34.0;
    let w = ui.available_width().min(galley.size().x + 52.0).max(galley.size().x + 52.0).min(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), if enabled { Sense::click() } else { Sense::hover() });
    if resp.hovered() && enabled {
        ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), HOVER);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = pos2(rect.left() + 16.0, rect.center().y);
    if selected {
        ui.painter().circle_filled(c, 4.0, if enabled { SELECTED } else { DIM });
    } else {
        ui.painter().circle_stroke(c, 3.5, Stroke::new(1.0_f32, if enabled { MUTED } else { DIM }));
    }
    ui.painter().galley(pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    resp
}

fn pill(ui: &mut Ui, text: &str, enabled: bool, primary: bool, min_w: f32) -> Response {
    let font = FontId::proportional(LABEL_SIZE);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, TEXT);
    let size = vec2((galley.size().x + 36.0).max(min_w), 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = ui.painter();
    let radius = CornerRadius::same(18);
    if primary {
        if enabled {
            // gradient pill: draw gradient then mask corners with circles
            let inner = rect.shrink2(vec2(18.0, 0.0));
            gradient_bar(p, inner, FILL_START, FILL_END);
            p.circle_filled(pos2(rect.left() + 18.0, rect.center().y), 18.0, FILL_START);
            p.circle_filled(pos2(rect.right() - 18.0, rect.center().y), 18.0, FILL_END);
            if hovered {
                p.rect_filled(rect, radius, Color32::from_white_alpha(18));
            }
        } else {
            p.rect_filled(rect, radius, FIELD);
        }
    } else {
        if hovered {
            p.rect_filled(rect, radius, HOVER);
        }
        p.rect_stroke(rect, radius, Stroke::new(1.0_f32, if enabled { OUTLINE } else { LINE }), StrokeKind::Inside);
    }
    let col = if !enabled { DIM } else if primary { Color32::WHITE } else { TEXT };
    p.galley(rect.center() - galley.size() / 2.0, galley, col);
    resp
}

pub fn button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    pill(ui, text, enabled, false, 0.0)
}

pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    pill(ui, text, enabled, true, 150.0)
}

/// Underlined caps tabs (like GAME / MIC). Returns true if changed.
pub fn tabs<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 26.0;
        for (v, label) in items {
            let sel = *current == *v;
            let job = spaced(&label.to_uppercase(), LABEL_SIZE, if sel { TEXT } else { MUTED }, false, 1.0);
            let galley = ui.fonts(|f| f.layout_job(job));
            let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x, galley.size().y + 10.0), Sense::click());
            let hovered = resp.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let color = if sel || hovered { TEXT } else { MUTED };
            ui.painter().galley(rect.left_top(), galley, color);
            if sel {
                let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), pos2(rect.right(), rect.bottom()));
                gradient_bar(ui.painter(), bar, FILL_START, FILL_END);
            }
            if resp.clicked() && !sel {
                *current = *v;
                changed = true;
            }
        }
    });
    changed
}

pub fn status_dot(ui: &mut Ui, color: Color32, text: &str) {
    // works in both layout directions: the dot always sits left of the text
    let rtl = ui.layout().prefer_right_to_left();
    let old = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 8.0;
    let dot = |ui: &mut Ui| {
        let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 3.5, color);
    };
    if rtl {
        ui.label(text);
        dot(ui);
    } else {
        dot(ui);
        ui.label(text);
    }
    ui.spacing_mut().item_spacing.x = old;
}

/// Small caps key on the left, value on the right (Device status rows).
pub fn kv(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        small_caps(ui, key, MUTED);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(value);
        });
    });
}

/// Colour swatch that opens a picker; `ring` draws the selection ring.
pub fn swatch(ui: &mut Ui, rgb: &mut [u8; 3], ring: bool, enabled: bool) -> Response {
    let size = vec2(30.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let p = ui.painter();
    if ring {
        p.rect_stroke(rect.expand(3.0), CornerRadius::same(RADIUS + 2), Stroke::new(2.0_f32, if enabled { SELECTED } else { DIM }), StrokeKind::Inside);
    }
    p.rect_filled(rect.shrink(1.0), CornerRadius::same(RADIUS), if enabled { col } else { dim(col) });
    p.rect_stroke(rect.shrink(1.0), CornerRadius::same(RADIUS), Stroke::new(1.0_f32, Color32::from_white_alpha(28)), StrokeKind::Inside);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Popup colour picker bound to a swatch response.
pub fn color_popup(ui: &mut Ui, resp: &Response, rgb: &mut [u8; 3]) -> bool {
    let id = resp.id.with("picker");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let mut changed = false;
    egui::popup::popup_below_widget(ui, id, resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(220.0);
        let mut c = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            *rgb = [c.r(), c.g(), c.b()];
            changed = true;
        }
        ui.horizontal(|ui| {
            small_caps(ui, "hex", MUTED);
            let mut hex = format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            if ui.add(egui::TextEdit::singleline(&mut hex).desired_width(80.0)).changed() {
                if let Ok(v) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
                    if hex.trim_start_matches('#').len() == 6 {
                        *rgb = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                        changed = true;
                    }
                }
            }
        });
    });
    changed
}

/// Swatch + popup picker in one call.
pub fn color_button(ui: &mut Ui, rgb: &mut [u8; 3], enabled: bool) -> bool {
    let r = swatch(ui, rgb, false, enabled);
    enabled && color_popup(ui, &r, rgb)
}

// ---------------------------------------------------------------- icons

#[derive(Clone, Copy)]
pub enum Icon {
    Mouse,
    Keyboard,
}

pub fn icon(painter: &egui::Painter, center: Pos2, which: Icon, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    match which {
        Icon::Mouse => {
            let r = Rect::from_center_size(center, vec2(13.0, 20.0));
            painter.rect_stroke(r, CornerRadius::same(6), s, StrokeKind::Middle);
            painter.line_segment([r.center_top() + vec2(0.0, 2.0), r.center_top() + vec2(0.0, 6.5)], s);
        }
        Icon::Keyboard => {
            let r = Rect::from_center_size(center, vec2(22.0, 14.0));
            painter.rect_stroke(r, CornerRadius::same(2), s, StrokeKind::Middle);
            for row in 0..2 {
                for col in 0..4 {
                    let p = r.left_top() + vec2(4.5 + col as f32 * 4.3, 4.0 + row as f32 * 3.5);
                    painter.circle_filled(p, 0.9, color);
                }
            }
            painter.line_segment([r.left_bottom() + vec2(6.0, -3.0), r.right_bottom() + vec2(-6.0, -3.0)], s);
        }
    }
}


/// Compact radio dot + short label at a fixed width (DPI stage rows).
pub fn fixed_radio(ui: &mut Ui, selected: bool, text: &str, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, 30.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), HOVER);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = pos2(rect.left() + 12.0, rect.center().y);
    let col = if !enabled { DIM } else if selected { SELECTED } else { MUTED };
    if selected {
        ui.painter().circle_filled(c, 4.0, col);
    } else {
        ui.painter().circle_stroke(c, 3.5, Stroke::new(1.0_f32, col));
    }
    let tc = if !enabled { DIM } else if selected { SELECTED } else { TEXT };
    ui.painter().text(pos2(rect.left() + 26.0, rect.center().y), egui::Align2::LEFT_CENTER, text, FontId::proportional(LABEL_SIZE), tc);
    resp
}

/// Selectable list row: caps title with a muted second line (like the Controls page).
pub fn two_line_item(ui: &mut Ui, selected: bool, title: &str, sub: &str) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 50.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(RADIUS), SELECTED_SOFT.gamma_multiply(0.55));
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(RADIUS), HOVER);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = pos2(rect.left() + 16.0, rect.top() + 17.0);
    if selected {
        p.circle_filled(c, 4.0, SELECTED);
    } else {
        p.circle_stroke(c, 3.5, Stroke::new(1.0_f32, MUTED));
    }
    let job = spaced(&title.to_uppercase(), LABEL_SIZE, if selected { SELECTED } else { TEXT }, false, 0.8);
    let g = ui.fonts(|f| f.layout_job(job));
    p.galley(pos2(rect.left() + 32.0, rect.top() + 8.0), g, TEXT);
    let sub_g = p.layout(sub.to_owned(), FontId::proportional(SMALL_SIZE), MUTED, w - 44.0);
    p.galley(pos2(rect.left() + 32.0, rect.top() + 28.0), sub_g, MUTED);
    resp
}

/// Small rounded status badge ("Live", "Active").
pub fn pill_badge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let g = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(SMALL_SIZE), fg);
    let size = vec2(g.size().x + 28.0, 26.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(13), bg);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
    resp
}
