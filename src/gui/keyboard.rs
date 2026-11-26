use super::mouse::modal;
use super::theme::*;
use super::widgets::{self as w, caps, card, hint, kv, radio_item, segmented, slider_row, toggle_row};
use super::{body, divider, footer, header, progress_bar, ComboPicker, Job};
use crate::keycodes;
use crate::talitha::{self, Key, LedConfig, Link as Transport, State};
use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, Layout, Pos2, Rect, Sense, Stroke, StrokeKind};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Lighting,
    PerKey,
    Keymap,
    Device,
}

enum Out {
    Config(LedConfig),
    Written(LedConfig),
    Nothing,
}

#[derive(PartialEq)]
enum Link {
    NoReceiver,
    Unknown,
    Asleep,
    Online,
}

pub struct KeyboardPage {
    keys: Vec<Key>,
    st: State,
    cfg: Option<LedConfig>,
    pub present: bool,
    transport: Option<Transport>,
    link: Link,
    tab: Tab,
    job: Option<Job<Out>>,
    status: Vec<String>,
    effect: usize,
    brightness: u8,
    speed: u8,
    color: u8,
    sleep: u8,
    paint: [u8; 3],
    eraser: bool,
    custom_brightness: u8,
    selected: Option<usize>,
    picker: Option<ComboPicker>,
    confirm_reset: bool,
    confirm_restore: bool,
}

impl KeyboardPage {
    pub fn new() -> Self {
        Self {
            keys: talitha::layout(),
            st: State::load(),
            cfg: None,
            present: false,
            transport: None,
            link: Link::NoReceiver,
            tab: Tab::Lighting,
            job: None,
            status: vec![],
            effect: 0,
            brightness: 4,
            speed: 4,
            color: 0,
            sleep: 0x0A,
            paint: [123, 69, 245],
            eraser: false,
            custom_brightness: 4,
            selected: None,
            picker: None,
            confirm_reset: false,
            confirm_restore: false,
        }
    }

    pub fn set_tab(&mut self, i: usize) {
        self.tab = [Tab::Lighting, Tab::PerKey, Tab::Keymap, Tab::Device][i.min(3)];
        if self.tab == Tab::Keymap && self.selected.is_none() {
            self.selected = self.keys.iter().find(|k| k.label == "Esc").map(|k| k.matrix);
        }
    }

    /// Ready for a screenshot: not busy, and lighting state loaded if the keyboard is there.
    pub fn settled(&self) -> bool {
        !self.busy() && (!self.present || self.cfg.is_some() || self.link == Link::Asleep)
    }

    pub fn take_status(&mut self) -> Vec<String> {
        std::mem::take(&mut self.status)
    }

    pub fn set_connected(&mut self, ctx: &egui::Context, transport: Option<Transport>) {
        let changed = transport != self.transport;
        self.transport = transport;
        self.present = transport.is_some();
        if !self.present {
            self.link = Link::NoReceiver;
            self.cfg = None;
        } else if changed {
            self.link = Link::Unknown;
            self.cfg = None;
            self.read(ctx);
        }
    }

    fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn can_write(&self) -> bool {
        self.present && !self.busy()
    }

    fn run<F>(&mut self, ctx: &egui::Context, f: F)
    where
        F: FnOnce(&talitha::Device, &dyn Fn(usize, usize)) -> std::io::Result<Out> + Send + 'static,
    {
        if self.busy() {
            return;
        }
        self.job = Some(Job::spawn(ctx, move |pr| {
            let d = talitha::Device::open()?;
            d.require_online()?;
            f(&d, pr)
        }));
    }

    fn read(&mut self, ctx: &egui::Context) {
        self.run(ctx, |d, _| Ok(Out::Config(d.read_config()?)));
    }

    fn load_effect(&mut self) {
        if let Some(c) = &self.cfg {
            let (b, s, col) = c.params(self.effect);
            self.brightness = b.min(talitha::BRIGHTNESS_MAX);
            self.speed = s.clamp(talitha::SPEED_MIN, talitha::SPEED_MAX);
            self.color = if col == talitha::RAINBOW { col } else { col.min(6) };
        }
    }

    fn poll_job(&mut self) {
        let Some(job) = &self.job else { return };
        let Some(r) = job.poll() else { return };
        self.job = None;
        match r {
            Ok(out @ (Out::Config(_) | Out::Written(_))) => {
                let (c, msg) = match out {
                    Out::Config(c) => (c, "Keyboard state read."),
                    Out::Written(c) => (c, "Keyboard updated."),
                    Out::Nothing => unreachable!(),
                };
                self.link = Link::Online;
                self.effect = c.effect();
                self.sleep = c.sleep();
                if self.effect == talitha::CUSTOM_EFFECT {
                    self.custom_brightness = c.params(self.effect).0.min(talitha::BRIGHTNESS_MAX);
                }
                self.cfg = Some(c);
                self.load_effect();
                self.status.push(msg.into());
            }
            Ok(Out::Nothing) => self.status.push("Keyboard updated.".into()),
            Err(e) => {
                if e.contains("asleep") || e.contains("offline") {
                    self.link = Link::Asleep;
                }
                self.status.push(format!("Keyboard: {e}"));
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.poll_job();
        let before = serde_json::to_string(&self.st).unwrap_or_default();

        footer(ctx, "kb-footer", |ui| self.footer(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BACKGROUND)).show(ctx, |ui| {
            let mut tab = self.tab;
            let via = self.transport.map(|t| t.name()).unwrap_or("—");
            let (dot, text) = match self.link {
                Link::NoReceiver => (DIM, "Not connected"),
                Link::Unknown => (WARN, "Connecting"),
                Link::Asleep => (WARN, "Keyboard asleep"),
                Link::Online => (GOOD, "Connected"),
            };
            header(
                ui,
                "TALITHA",
                "KEYBOARD",
                |ui| {
                    w::tabs(ui, &mut tab, &[
                        (Tab::Lighting, "Lighting"),
                        (Tab::PerKey, "Per-key colours"),
                        (Tab::Keymap, "Key mapping"),
                        (Tab::Device, "Device"),
                    ]);
                },
                |ui| {
                    w::status_dot(ui, dot, text);
                    divider(ui);
                    ui.label(via);
                    w::small_caps(ui, "Link", MUTED);
                },
            );
            self.tab = tab;
            body(ui, |ui| {
                if self.link == Link::Asleep {
                    banner(ui, "The keyboard is asleep or out of range. Press any key, then choose Read from keyboard.");
                }
                match self.tab {
                    Tab::Lighting => self.lighting(ui),
                    Tab::PerKey => self.per_key(ui),
                    Tab::Keymap => self.keymap(ui),
                    Tab::Device => self.device(ui),
                }
            });
        });

        self.dialogs(ctx);
        if serde_json::to_string(&self.st).unwrap_or_default() != before {
            self.st.save();
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        if w::button(ui, "Read from keyboard", self.can_write()).clicked() {
            let ctx = ui.ctx().clone();
            self.read(&ctx);
        }
        if self.tab == Tab::Keymap && w::button(ui, "Restore default layout", self.can_write()).clicked() {
            self.confirm_restore = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (label, enabled) = match self.tab {
                Tab::Lighting => ("Apply lighting", self.can_write() && self.cfg.is_some()),
                Tab::PerKey => ("Apply colours", self.can_write()),
                Tab::Keymap => ("Apply key mapping", self.can_write()),
                Tab::Device => ("", false),
            };
            if !label.is_empty() && w::primary_button(ui, label, enabled).clicked() {
                match self.tab {
                    Tab::Lighting => self.apply_lighting(ui.ctx()),
                    Tab::PerKey => self.apply_custom(ui.ctx()),
                    Tab::Keymap => {
                        let m = talitha::build_matrix(&self.st.keymap, &self.keys);
                        self.run(ui.ctx(), move |d, pr| {
                            d.write_matrix(&m, 0, pr)?;
                            Ok(Out::Nothing)
                        });
                    }
                    Tab::Device => {}
                }
            }
            if let Some(j) = &self.job {
                progress_bar(ui, j.progress());
            } else if self.transport == Some(Transport::Bluetooth) {
                w::muted(ui, "Bluetooth reads can take up to 20 s on lossy adapters.");
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(p) = &mut self.picker {
            if let Some(res) = p.show(ctx) {
                if let (Some((m, u)), Some(sel)) = (res, self.selected) {
                    self.assign(sel, Some(talitha::keyboard_code(m, u)));
                }
                self.picker = None;
            }
        }
        if self.confirm_restore {
            modal(ctx, "Restore layout", |ui| {
                ui.label("Clear all remaps and write the default layout to the keyboard?");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if w::primary_button(ui, "Restore", true).clicked() {
                        self.st.keymap.clear();
                        let m = talitha::build_matrix(&self.st.keymap, &self.keys);
                        self.confirm_restore = false;
                        self.run(ui.ctx(), move |d, pr| {
                            d.write_matrix(&m, 0, pr)?;
                            Ok(Out::Nothing)
                        });
                    }
                    if w::button(ui, "Cancel", true).clicked() {
                        self.confirm_restore = false;
                    }
                });
            });
        }
        if self.confirm_reset {
            modal(ctx, "Factory reset", |ui| {
                ui.label("Reset the keyboard's lighting, key mapping and settings to factory defaults?");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if w::primary_button(ui, "Reset", true).clicked() {
                        self.confirm_reset = false;
                        self.st.keymap.clear();
                        self.run(ui.ctx(), |d, _| {
                            d.factory_reset()?;
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            Ok(Out::Config(d.read_config()?))
                        });
                    }
                    if w::button(ui, "Cancel", true).clicked() {
                        self.confirm_reset = false;
                    }
                });
            });
        }
    }

    // ------------------------------------------------------------ lighting
    fn apply_lighting(&mut self, ctx: &egui::Context) {
        let Some(mut cfg) = self.cfg.clone() else { return };
        let fx = &talitha::EFFECTS[self.effect];
        let color = if fx.colors { self.color } else { cfg.params(self.effect).2 };
        cfg.set_effect(self.effect, self.brightness, self.speed, color);
        cfg.set_sleep(self.sleep);
        let palettes = self.st.palettes.clone();
        let write_pal = fx.colors && color != talitha::RAINBOW;
        self.run(ctx, move |d, pr| {
            if write_pal {
                d.write_palette(&palettes, pr)?;
            }
            d.write_config(&cfg, pr)?;
            Ok(Out::Written(cfg))
        });
    }

    fn lighting(&mut self, ui: &mut egui::Ui) {
        if self.cfg.is_none() {
            card(ui, "Lighting", |ui| {
                hint(ui, if self.busy() { "Reading the keyboard…" } else { "Read the keyboard first (it has to be awake) to edit its lighting." });
            });
            return;
        }
        let prev = self.effect;
        ui.columns(2, |cols| {
            card(&mut cols[0], "Effect", |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let list: Vec<usize> = (0..talitha::EFFECTS.len()).filter(|i| *i != talitha::HIDDEN_EFFECT).collect();
                let half = list.len().div_ceil(2);
                ui.columns(2, |c| {
                    for (n, &i) in list.iter().enumerate() {
                        let name = talitha::EFFECTS[i].name.split(" (").next().unwrap_or("");
                        if radio_item(&mut c[n / half], self.effect == i, name, true).clicked() {
                            self.effect = i;
                        }
                    }
                });
            });
            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = 24.0;
            let fx = &talitha::EFFECTS[self.effect];
            card(ui, "Parameters", |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let b = self.brightness;
                slider_row(ui, "Brightness", &mut self.brightness, 0..=talitha::BRIGHTNESS_MAX, "0", "4", Some(format!("{b}")), fx.brightness);
                ui.add_space(6.0);
                let s = self.speed;
                slider_row(ui, "Speed", &mut self.speed, talitha::SPEED_MIN..=talitha::SPEED_MAX, "1", "4", Some(format!("{s}")), fx.speed);
            });
            card(ui, "Colour", |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                let mut rainbow = self.color == talitha::RAINBOW;
                if toggle_row(ui, "Colour mixing", &mut rainbow, fx.colors) {
                    self.color = if rainbow { talitha::RAINBOW } else { 0 };
                }
                let en = fx.colors && !rainbow;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let pal = &mut self.st.palettes[self.effect];
                    for k in 0..7 {
                        let chosen = self.color == k as u8;
                        let r = w::swatch(ui, &mut pal[k], en && chosen, en);
                        if en && chosen {
                            // a second click on the chosen slot opens the picker
                            w::color_popup(ui, &r, &mut pal[k]);
                        } else if r.clicked() && en {
                            self.color = k as u8;
                        }
                    }
                });
                hint(ui, if fx.colors {
                    "Pick a slot, then click it again to change its colour. Each effect keeps its own seven colours."
                } else {
                    "This effect uses its own colours."
                });
            });
            card(ui, "Power", |ui| {
                caps(ui, "Sleep after", TEXT);
                let opts: Vec<(u8, &str)> = talitha::SLEEP_OPTIONS.iter().map(|(v, n)| (*v, *n)).collect();
                let wdt = ui.available_width();
                segmented(ui, &mut self.sleep, &opts, true, wdt);
            });
        });
        if prev != self.effect {
            self.load_effect();
        }
    }

    // ------------------------------------------------------------ keyboard drawing
    fn draw_keyboard(
        &self,
        ui: &mut egui::Ui,
        fill: &dyn Fn(&Key) -> Option<Color32>,
        label: &dyn Fn(&Key) -> String,
        marked: &dyn Fn(&Key) -> bool,
    ) -> Option<usize> {
        let x0 = self.keys.iter().map(|k| k.rect[0]).min().unwrap_or(0) - 14;
        let y0 = self.keys.iter().filter(|k| k.rect[3] > 0).map(|k| k.rect[1]).min().unwrap_or(0) - 14;
        let wdt = (self.keys.iter().map(|k| k.rect[2]).max().unwrap_or(1) - x0 + 14) as f32;
        let hgt = (self.keys.iter().map(|k| k.rect[3]).max().unwrap_or(1) - y0 + 14) as f32;
        let scale = (ui.available_width() / wdt).min(1.45);
        let size = vec2(wdt * scale, hgt * scale);
        let (outer, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::click());
        let rect = Rect::from_center_size(outer.center(), size);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(10), INSET);
        painter.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
        let hover = resp.hover_pos();
        let mut clicked = None;
        for k in &self.keys {
            if k.rect[2] == 0 {
                continue;
            }
            // the vendor rectangles jitter by a pixel or two; snap them to their row
            let y1 = snap_row(k.rect[1]);
            let y2 = y1 + (k.rect[3] - k.rect[1]);
            let r = Rect::from_min_max(
                rect.min + vec2((k.rect[0] - x0) as f32 * scale, (y1 - y0) as f32 * scale),
                rect.min + vec2((k.rect[2] - x0) as f32 * scale, (y2 - y0) as f32 * scale),
            )
            .shrink(2.0);
            let custom = fill(k);
            let base = custom.unwrap_or(c(0x2a3039));
            let hovered = hover.is_some_and(|p: Pos2| r.contains(p));
            // key cap: darker skirt and a lighter top face
            painter.rect_filled(r, CornerRadius::same(5), base.gamma_multiply(0.72));
            let top = Rect::from_min_max(r.min + vec2(2.0, 1.5), r.max - vec2(2.0, 4.0));
            let face = if custom.is_some() { base } else if hovered { c(0x3b4350) } else { c(0x323944) };
            painter.rect_filled(top, CornerRadius::same(4), face);
            let sel = self.selected == Some(k.matrix) && self.tab == Tab::Keymap;
            if sel {
                painter.rect_stroke(r.expand(1.0), CornerRadius::same(6), Stroke::new(2.0_f32, SELECTED), StrokeKind::Outside);
            } else if marked(k) {
                painter.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.5_f32, ACCENT), StrokeKind::Outside);
            } else if hovered {
                painter.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0_f32, OUTLINE), StrokeKind::Outside);
            }
            let lum = 0.299 * base.r() as f32 + 0.587 * base.g() as f32 + 0.114 * base.b() as f32;
            let tc = if custom.is_some() && lum > 150.0 {
                c(0x111419)
            } else if custom.is_some() || marked(k) || sel {
                TEXT
            } else {
                MUTED
            };
            // one line, shrunk to fit the cap rather than wrapped mid-word
            let text = short_label(&label(k));
            let mut size = (10.0 * scale).clamp(8.5, 13.0);
            let mut galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(size), tc);
            while galley.size().x > top.width() - 6.0 && size > 7.0 {
                size -= 0.5;
                galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(size), tc);
            }
            painter.galley(top.center() - galley.size() / 2.0, galley, tc);
            if resp.clicked() && hovered {
                clicked = Some(k.matrix);
            }
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        clicked
    }

    // ------------------------------------------------------------ per-key colours
    fn apply_custom(&mut self, ctx: &egui::Context) {
        let colors = self.st.custom_colors.clone();
        let b = self.custom_brightness;
        let known = self.cfg.clone();
        self.run(ctx, move |d, pr| {
            let mut cfg = match known {
                Some(c) => c,
                None => d.read_config()?,
            };
            d.write_custom_colors(&colors, pr)?;
            cfg.set_effect(talitha::CUSTOM_EFFECT, b, 0, 0);
            d.write_config(&cfg, pr)?;
            Ok(Out::Written(cfg))
        });
    }

    fn per_key(&mut self, ui: &mut egui::Ui) {
        card(ui, "Per-key colours", |ui| {
            let colors = self.st.custom_colors.clone();
            let fill = |k: &Key| colors.get(&k.matrix).map(|c| Color32::from_rgb(c[0], c[1], c[2]));
            let label = |k: &Key| k.label.clone();
            if let Some(m) = self.draw_keyboard(ui, &fill, &label, &|_| false) {
                if self.eraser {
                    self.st.custom_colors.remove(&m);
                } else {
                    self.st.custom_colors.insert(m, self.paint);
                }
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                caps(ui, "Paint", TEXT);
                w::color_button(ui, &mut self.paint, true);
                for preset in [[123u8, 69, 245], [226, 69, 156], [61, 214, 184], [255, 255, 255], [255, 80, 40], [40, 120, 255]] {
                    let mut col = preset;
                    if w::swatch(ui, &mut col, self.paint == preset && !self.eraser, true).clicked() {
                        self.paint = preset;
                        self.eraser = false;
                    }
                }
                ui.add_space(10.0);
                caps(ui, "Eraser", TEXT);
                w::toggle(ui, &mut self.eraser, true);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::button(ui, "Clear all", true).clicked() {
                        self.st.custom_colors.clear();
                    }
                    if w::button(ui, "Fill all", true).clicked() {
                        for k in &self.keys {
                            self.st.custom_colors.insert(k.matrix, self.paint);
                        }
                    }
                });
            });
            ui.add_space(8.0);
            let b = self.custom_brightness;
            ui.allocate_ui(vec2(ui.available_width().min(420.0), 60.0), |ui| {
                slider_row(ui, "Brightness", &mut self.custom_brightness, 0..=talitha::BRIGHTNESS_MAX, "0", "4", Some(format!("{b}")), true);
            });
            hint(ui, "Click keys to paint them. Applying switches the keyboard to the Self-define effect.");
        });
    }

    // ------------------------------------------------------------ key mapping
    fn assign(&mut self, m: usize, code: Option<talitha::Code>) {
        let def = self.keys.iter().find(|k| k.matrix == m).map(talitha::default_code);
        match code {
            Some(c) if Some(c) != def => {
                self.st.keymap.insert(m, c);
            }
            _ => {
                self.st.keymap.remove(&m);
            }
        }
    }

    fn keymap(&mut self, ui: &mut egui::Ui) {
        let km = self.st.keymap.clone();
        card(ui, "Key mapping", |ui| {
            let label = |k: &Key| km.get(&k.matrix).map(|c| talitha::describe_code(*c)).unwrap_or_else(|| k.label.clone());
            let marked = |k: &Key| km.contains_key(&k.matrix);
            if let Some(m) = self.draw_keyboard(ui, &|_| None, &label, &marked) {
                self.selected = Some(m);
            }
            ui.add_space(8.0);
            hint(ui, "Remaps the base layer; Fn-layer functions are kept. A purple outline marks a remapped key.");
        });
        let sel = self.selected.and_then(|m| self.keys.iter().find(|k| k.matrix == m)).cloned();
        card(ui, "Assignment", |ui| match sel {
            None => hint(ui, "Select a key on the keyboard above."),
            Some(k) => {
                let cur = km.get(&k.matrix).copied();
                let desc = talitha::describe_code(cur.unwrap_or_else(|| talitha::default_code(&k)));
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(w::spaced(&k.label.to_uppercase(), TITLE_SIZE, SELECTED, false, 1.0)));
                    w::small_caps(ui, "maps to", MUTED);
                    ui.label(egui::RichText::new(&desc).size(TITLE_SIZE));
                    if cur.is_none() {
                        w::pill_badge(ui, "Default", MUTED, FIELD);
                    }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if w::button(ui, "Keyboard key…", true).clicked() {
                        self.picker = Some(ComboPicker::new(format!("Remap {}", k.label)));
                    }
                    let mut media: Option<u16> = None;
                    egui::ComboBox::from_id_salt("media").selected_text("Media / app key").width(190.0).show_ui(ui, |ui| {
                        for (u, n) in keycodes::CONSUMER.iter().filter(|c| !c.1.is_empty()) {
                            if ui.selectable_label(false, *n).clicked() {
                                media = Some(*u);
                            }
                        }
                    });
                    if let Some(u) = media {
                        self.assign(k.matrix, Some(talitha::consumer_code(u)));
                    }
                    let mut mouse: Option<u8> = None;
                    egui::ComboBox::from_id_salt("mousebtn").selected_text("Mouse button").width(160.0).show_ui(ui, |ui| {
                        for (n, m) in talitha::MOUSE_BUTTONS {
                            if ui.selectable_label(false, n).clicked() {
                                mouse = Some(m);
                            }
                        }
                    });
                    if let Some(m) = mouse {
                        self.assign(k.matrix, Some(talitha::mouse_code(m)));
                    }
                    if w::button(ui, "Disable", true).clicked() {
                        self.assign(k.matrix, Some(talitha::DISABLED));
                    }
                    if w::button(ui, "Default", cur.is_some()).clicked() {
                        self.assign(k.matrix, None);
                    }
                });
            }
        });
    }

    // ------------------------------------------------------------ device
    fn device(&mut self, ui: &mut egui::Ui) {
        let via = self.transport.map(|t| t.name()).unwrap_or("not connected");
        let state = match self.link {
            Link::NoReceiver => "—",
            Link::Unknown => "checking",
            Link::Asleep => "asleep / out of range",
            Link::Online => "online",
        };
        ui.columns(2, |cols| {
            card(&mut cols[0], "Device status", |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                kv(ui, "Link", via);
                kv(ui, "Keyboard", state);
                kv(ui, "Receiver", if talitha::find_dongle().is_some() { "3554:fa09 plugged in" } else { "not plugged in" });
                kv(ui, "Bluetooth", if talitha::find_bluetooth().is_some() { "connected" } else { "not connected" });
                if let Some(c) = &self.cfg {
                    kv(ui, "Effect", talitha::EFFECTS[c.effect()].name);
                    let sleep = talitha::SLEEP_OPTIONS.iter().find(|o| o.0 == c.sleep()).map(|o| o.1).unwrap_or("custom");
                    kv(ui, "Sleep", sleep);
                }
                kv(ui, "Remapped keys", &self.st.keymap.len().to_string());
                ui.add_space(4.0);
                hint(ui, "When the keyboard is connected over Bluetooth, the app uses Bluetooth; otherwise it uses the 2.4 GHz receiver.");
            });
            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = 24.0;
            card(ui, "Factory reset", |ui| {
                hint(ui, "Restores the keyboard's lighting, key mapping and settings to factory defaults.");
                ui.add_space(8.0);
                if w::button(ui, "Factory reset keyboard…", self.can_write()).clicked() {
                    self.confirm_reset = true;
                }
            });
            card(ui, "About", |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(concat!("PolariTha ", env!("CARGO_PKG_VERSION")));
                hint(ui, "An unofficial Linux configurator for the AQIRYS Talitha and Polaris Wired, not affiliated with AQIRYS.");
                hint(ui, "Protocols reverse engineered from the vendor's Windows tools; see docs/PROTOCOL.md.");
            });
        });
    }
}

/// Rows of the vendor layout (y of the key tops).
fn snap_row(y: i32) -> i32 {
    const ROWS: [i32; 5] = [38, 85, 133, 181, 229];
    *ROWS.iter().min_by_key(|r| (y - **r).abs()).filter(|r| (y - **r).abs() <= 4).unwrap_or(&y)
}

fn short_label(s: &str) -> String {
    match s {
        "Delete" => "Del".into(),
        "CapsLock" => "Caps".into(),
        "Backspace" => "Bksp".into(),
        other => other.to_string(),
    }
}

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Amber notice strip.
fn banner(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(WARN_SOFT)
        .stroke(Stroke::new(1.0_f32, c(0x4a3c1c)))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
                ui.painter().circle_filled(pos2(r.center().x, r.center().y), 3.5, WARN);
                ui.label(egui::RichText::new(text).color(WARN));
            });
        });
}
