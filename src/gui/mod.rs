//! egui front-end, styled after swarm2linux.

mod keyboard;
mod mouse;
pub mod theme;
pub mod widgets;

use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, Layout, Rect, Sense};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use theme::*;
use widgets::Icon;

/// A device operation running on a worker thread.
pub struct Job<T> {
    shared: Arc<Mutex<JobShared<T>>>,
}

struct JobShared<T> {
    progress: (usize, usize),
    result: Option<Result<T, String>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn<F>(ctx: &egui::Context, f: F) -> Self
    where
        F: FnOnce(&dyn Fn(usize, usize)) -> std::io::Result<T> + Send + 'static,
    {
        let shared = Arc::new(Mutex::new(JobShared { progress: (0, 0), result: None }));
        let s2 = shared.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let c2 = ctx.clone();
            let s3 = s2.clone();
            let progress = move |d: usize, t: usize| {
                s3.lock().unwrap().progress = (d, t);
                c2.request_repaint();
            };
            let r = f(&progress).map_err(|e| e.to_string());
            s2.lock().unwrap().result = Some(r);
            ctx.request_repaint();
        });
        Self { shared }
    }

    pub fn progress(&self) -> (usize, usize) {
        self.shared.lock().unwrap().progress
    }

    /// Takes the result once finished.
    pub fn poll(&self) -> Option<Result<T, String>> {
        self.shared.lock().unwrap().result.take()
    }
}

/// Thin gradient progress line with a caption.
pub fn progress_bar(ui: &mut egui::Ui, p: (usize, usize)) {
    let (rect, _) = ui.allocate_exact_size(vec2(160.0, 36.0), Sense::hover());
    let bar = Rect::from_min_size(pos2(rect.left(), rect.center().y - 1.5), vec2(rect.width(), 3.0));
    ui.painter().rect_filled(bar, CornerRadius::same(2), TRACK);
    let frac = if p.1 == 0 {
        // indeterminate: a moving segment
        let t = (ui.input(|i| i.time) * 0.8).fract() as f32;
        ui.ctx().request_repaint();
        let w = bar.width() * 0.3;
        let x = bar.left() + (bar.width() + w) * t - w;
        let seg = Rect::from_min_max(pos2(x.max(bar.left()), bar.top()), pos2((x + w).min(bar.right()), bar.bottom()));
        ui.painter().rect_filled(seg, CornerRadius::same(2), ACCENT);
        None
    } else {
        Some(p.0 as f32 / p.1 as f32)
    };
    if let Some(f) = frac {
        let fill = Rect::from_min_size(bar.min, vec2(bar.width() * f, bar.height()));
        ui.painter().rect_filled(fill, CornerRadius::same(2), ACCENT);
        ui.painter().text(
            pos2(rect.left(), rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            format!("{} / {}", p.0, p.1),
            egui::FontId::proportional(SMALL_SIZE - 1.0),
            MUTED,
        );
    }
}

/// Page header: two-tone title, tabs underneath, status on the right.
pub fn header(ui: &mut egui::Ui, bold: &str, light: &str, tabs: impl FnOnce(&mut egui::Ui), right: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(24.0);
    ui.horizontal_top(|ui| {
        ui.add_space(PAD);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.add(egui::Label::new(widgets::spaced(bold, 26.0, TEXT, true, 1.0)).selectable(false));
                ui.add(egui::Label::new(widgets::spaced(light, 26.0, MUTED, false, 1.0)).selectable(false));
            });
            ui.add_space(2.0);
            tabs(ui);
        });
        let avail = ui.available_width();
        ui.allocate_ui_with_layout(vec2(avail, 40.0), Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(PAD);
            ui.spacing_mut().item_spacing.x = 14.0;
            right(ui);
        });
    });
    ui.add_space(14.0);
}

/// Vertical divider used between header status items.
pub fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 40.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, LINE);
}

/// Page content area with the standard side padding and a scroll area.
pub fn body(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: PAD as i8, right: PAD as i8, top: 0, bottom: 24 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(24.0, 24.0);
            add(ui);
        });
    });
}

/// Bottom action bar.
pub fn footer(ctx: &egui::Context, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::TopBottomPanel::bottom(id.to_owned())
        .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(egui::Margin { left: PAD as i8, right: PAD as i8, top: 12, bottom: 16 }))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([pos2(r.left(), r.top() - 12.0), pos2(r.right(), r.top() - 12.0)], egui::Stroke::new(1.0_f32, LINE));
            ui.horizontal(add);
        });
}

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Mouse,
    Keyboard,
}

/// POLARITHA_SCREENSHOTS=<dir>: capture every page/tab to PNGs, then quit.
struct Shots {
    dir: std::path::PathBuf,
    queue: Vec<(Page, usize, &'static str)>,
    since: Instant,
    requested: bool,
}

struct App {
    shots: Option<Shots>,
    page: Page,
    mouse: mouse::MousePage,
    keyboard: keyboard::KeyboardPage,
    last_poll: Instant,
    toast: Option<(String, Instant)>,
    logo: egui::TextureHandle,
}

const ICON_PNG: &[u8] = include_bytes!("../../assets/icons/polaritha-128.png");

fn decode_icon() -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).expect("icon png").into_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        theme::install(&cc.egui_ctx);
        let shots = std::env::var_os("POLARITHA_SCREENSHOTS").map(|d| Shots {
            dir: d.into(),
            queue: vec![
                (Page::Mouse, 0, "mouse-performance"),
                (Page::Mouse, 1, "mouse-lighting"),
                (Page::Mouse, 2, "mouse-buttons"),
                (Page::Mouse, 3, "mouse-profiles"),
                (Page::Keyboard, 0, "keyboard-lighting"),
                (Page::Keyboard, 1, "keyboard-per-key"),
                (Page::Keyboard, 2, "keyboard-keymap"),
                (Page::Keyboard, 3, "keyboard-device"),
            ],
            since: Instant::now(),
            requested: false,
        });
        Self {
            shots,
            page: Page::Mouse,
            mouse: mouse::MousePage::new(),
            keyboard: keyboard::KeyboardPage::new(),
            last_poll: Instant::now() - Duration::from_secs(10),
            toast: None,
            logo: {
                let (rgba, w, h) = decode_icon();
                let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
            },
        }
    }

    fn rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("rail")
            .exact_width(84.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(RAIL))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().line_segment([full.right_top(), full.right_bottom()], egui::Stroke::new(1.0_f32, LINE));
                let (logo, _) = ui.allocate_exact_size(vec2(84.0, 74.0), Sense::hover());
                let r = egui::Rect::from_center_size(logo.center() + vec2(0.0, 6.0), vec2(44.0, 44.0));
                ui.painter().image(self.logo.id(), r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                for (p, icon, label, online) in [
                    (Page::Mouse, Icon::Mouse, "Mouse", self.mouse.connected),
                    (Page::Keyboard, Icon::Keyboard, "Keyboard", self.keyboard.present),
                ] {
                    let (r, resp) = ui.allocate_exact_size(vec2(84.0, 66.0), Sense::click());
                    let active = self.page == p;
                    let bg = if active { RAIL_ACTIVE } else if resp.hovered() { RAIL_HOVER } else { Color32::TRANSPARENT };
                    ui.painter().rect_filled(r, CornerRadius::ZERO, bg);
                    if active {
                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), CornerRadius::ZERO, ACCENT);
                    }
                    let col = if active || resp.hovered() { TEXT } else { MUTED };
                    widgets::icon(ui.painter(), r.center() - vec2(0.0, 9.0), icon, col);
                    ui.painter().text(r.center() + vec2(0.0, 15.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), col);
                    // connection dot
                    ui.painter().circle_filled(r.center() + vec2(15.0, -18.0), 3.0, if online { GOOD } else { DIM });
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        self.page = p;
                    }
                }
            });
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some((msg, t)) = &self.toast else { return };
        let age = t.elapsed().as_secs_f32();
        if age > 6.0 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        let alpha = ((6.0 - age) / 0.5).clamp(0.0, 1.0);
        let is_err = msg.contains("error") || msg.contains(": ") && !msg.ends_with('.');
        egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, [42.0, -84.0]).interactable(false).show(ctx, |ui| {
            egui::Frame::new()
                .fill(POPUP.gamma_multiply(alpha))
                .stroke(egui::Stroke::new(1.0_f32, (if is_err { BAD } else { LINE }).gamma_multiply(alpha)))
                .corner_radius(CornerRadius::same(RADIUS))
                .inner_margin(egui::Margin::symmetric(18, 10))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(msg).color(TEXT.gamma_multiply(alpha)));
                });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_poll.elapsed() > Duration::from_secs(2) {
            self.last_poll = Instant::now();
            self.mouse.set_connected(crate::polaris::find_device().is_some());
            self.keyboard.set_connected(ctx, crate::talitha::find_device().map(|(_, l)| l));
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        for msg in self.mouse.take_status().into_iter().chain(self.keyboard.take_status()) {
            self.toast = Some((msg, Instant::now()));
        }
        self.rail(ctx);
        match self.page {
            Page::Mouse => self.mouse.show(ctx),
            Page::Keyboard => self.keyboard.show(ctx),
        }
        if self.shots.is_none() {
            self.draw_toast(ctx);
        }
        self.screenshots(ctx);
    }
}

impl App {
    fn screenshots(&mut self, ctx: &egui::Context) {
        let Some(sh) = &mut self.shots else { return };
        // a finished capture?
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let (_, _, name) = sh.queue.remove(0);
            let _ = std::fs::create_dir_all(&sh.dir);
            let path = sh.dir.join(format!("{name}.png"));
            let buf: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(rgba) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, buf) {
                let _ = rgba.save(&path);
                eprintln!("saved {}", path.display());
            }
            sh.requested = false;
            sh.since = Instant::now();
        }
        let Some(&(page, tab, _)) = sh.queue.first() else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.page = page;
        match page {
            Page::Mouse => self.mouse.set_tab(tab),
            Page::Keyboard => self.keyboard.set_tab(tab),
        }
        let settled = page == Page::Mouse || self.keyboard.settled() || sh.since.elapsed() > Duration::from_secs(60);
        if !sh.requested && settled && sh.since.elapsed() > Duration::from_millis(1200) {
            sh.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

pub fn run() -> Result<(), String> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PolariTha")
            .with_app_id("polaritha")
            .with_icon({
                let (rgba, width, height) = decode_icon();
                Arc::new(egui::IconData { rgba, width, height })
            })
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([1000.0, 660.0]),
        ..Default::default()
    };
    eframe::run_native("PolariTha", opts, Box::new(|cc| Ok(Box::new(App::new(cc))))).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- shared widgets

/// egui key -> HID usage (for "press a key" capture).
pub fn egui_key_to_hid(k: egui::Key) -> Option<u8> {
    use egui::Key::*;
    let name = k.name();
    if name.len() == 1 {
        let c = name.chars().next().unwrap();
        if c.is_ascii_uppercase() {
            return Some(0x04 + (c as u8 - b'A'));
        }
    }
    Some(match k {
        Num1 => 0x1E, Num2 => 0x1F, Num3 => 0x20, Num4 => 0x21, Num5 => 0x22,
        Num6 => 0x23, Num7 => 0x24, Num8 => 0x25, Num9 => 0x26, Num0 => 0x27,
        Enter => 0x28, Escape => 0x29, Backspace => 0x2A, Tab => 0x2B, Space => 0x2C,
        Minus => 0x2D, Equals => 0x2E, OpenBracket => 0x2F, CloseBracket => 0x30,
        Backslash => 0x31, Semicolon => 0x33, Quote => 0x34, Backtick => 0x35,
        Comma => 0x36, Period => 0x37, Slash => 0x38,
        F1 => 0x3A, F2 => 0x3B, F3 => 0x3C, F4 => 0x3D, F5 => 0x3E, F6 => 0x3F,
        F7 => 0x40, F8 => 0x41, F9 => 0x42, F10 => 0x43, F11 => 0x44, F12 => 0x45,
        Insert => 0x49, Home => 0x4A, PageUp => 0x4B, Delete => 0x4C, End => 0x4D,
        PageDown => 0x4E, ArrowRight => 0x4F, ArrowLeft => 0x50, ArrowDown => 0x51, ArrowUp => 0x52,
        _ => return None,
    })
}

/// Modal-ish window to pick a key combination. Returns Some(Some(..)) on OK,
/// Some(None) on cancel, None while still open.
pub struct ComboPicker {
    pub title: String,
    pub mods: u8,
    pub usage: u8,
}

impl ComboPicker {
    pub fn new(title: String) -> Self {
        Self { title, mods: 0, usage: 0 }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Option<(u8, u8)>> {
        let mut out = None;
        egui::Window::new(&self.title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Press the key combination, or choose it below.");
            ctx.input(|i| {
                for ev in &i.events {
                    if let egui::Event::Key { key, pressed: true, modifiers, .. } = ev {
                        if let Some(h) = egui_key_to_hid(*key) {
                            self.usage = h;
                            self.mods = (modifiers.ctrl as u8) | ((modifiers.shift as u8) << 1) | ((modifiers.alt as u8) << 2);
                        }
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(crate::keycodes::describe_combo(self.mods, self.usage)).heading());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (bit, name) in crate::keycodes::MODIFIERS {
                    let mut on = self.mods & bit != 0;
                    if ui.checkbox(&mut on, name).changed() {
                        self.mods ^= bit;
                    }
                }
            });
            egui::ComboBox::from_id_salt("keypick")
                .selected_text(if self.usage == 0 { "(no key)".into() } else { crate::keycodes::key_name(self.usage) })
                .height(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.usage, 0, "(no key)");
                    for u in crate::keycodes::all_keys() {
                        ui.selectable_value(&mut self.usage, u, crate::keycodes::key_name(u));
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.usage != 0 || self.mods != 0, egui::Button::new("OK")).clicked() {
                    out = Some(Some((self.mods, self.usage)));
                }
                if ui.button("Cancel").clicked() {
                    out = Some(None);
                }
            });
        });
        out
    }
}
