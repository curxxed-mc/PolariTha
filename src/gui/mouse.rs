use super::theme::*;
use super::widgets::{self as w, caps, card, hint, kv, radio_item, segmented, slider, slider_row, toggle_row};
use super::{body, divider, footer, header, progress_bar, ComboPicker, Job};
use crate::polaris::{self, Settings, NUM_PROFILES, NUM_STAGES, NUM_USER_BUTTONS};
use eframe::egui::{self, vec2, Align, Layout};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Performance,
    Lighting,
    Buttons,
    Profiles,
}

pub struct MousePage {
    s: Settings,
    profile: usize,
    tab: Tab,
    pub connected: bool,
    job: Option<Job<()>>,
    status: Vec<String>,
    picker: Option<(usize, ComboPicker)>,
    confirm_reset: bool,
    xy_sync: bool,
    button: usize,
}

impl MousePage {
    pub fn new() -> Self {
        let s = Settings::load();
        let profile = s.active_profile;
        Self {
            s,
            profile,
            tab: Tab::Performance,
            connected: false,
            job: None,
            status: vec![],
            picker: None,
            confirm_reset: false,
            xy_sync: true,
            button: 0,
        }
    }

    pub fn set_tab(&mut self, i: usize) {
        self.tab = [Tab::Performance, Tab::Lighting, Tab::Buttons, Tab::Profiles][i.min(3)];
    }

    pub fn set_connected(&mut self, c: bool) {
        self.connected = c;
    }

    pub fn take_status(&mut self) -> Vec<String> {
        std::mem::take(&mut self.status)
    }

    fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            if let Some(r) = job.poll() {
                self.status.push(match r {
                    Ok(()) => "Mouse settings written.".into(),
                    Err(e) => format!("Mouse: {e}"),
                });
                self.job = None;
            }
        }
        let before = serde_json::to_string(&self.s).unwrap_or_default();

        footer(ctx, "mouse-footer", |ui| {
            if self.tab == Tab::Profiles {
                if w::button(ui, "Reset profile", true).clicked() {
                    self.confirm_reset = true;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if w::primary_button(ui, "Apply to mouse", self.connected && !self.busy()).clicked() {
                    let s = self.s.clone();
                    self.job = Some(Job::spawn(ui.ctx(), move |pr| polaris::Device::open()?.apply(&s, pr)));
                }
                if let Some(j) = &self.job {
                    progress_bar(ui, j.progress());
                } else if !self.connected {
                    w::muted(ui, "Connect the mouse to apply.");
                } else {
                    w::muted(ui, "Changes are saved on this PC and written to all 5 profiles on Apply.");
                }
            });
        });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BACKGROUND)).show(ctx, |ui| {
            let connected = self.connected;
            let pname = self.s.profiles[self.profile].name.clone();
            let mut tab = self.tab;
            header(
                ui,
                "POLARIS",
                "WIRED",
                |ui| {
                    w::tabs(ui, &mut tab, &[
                        (Tab::Performance, "Performance"),
                        (Tab::Lighting, "Lighting"),
                        (Tab::Buttons, "Buttons"),
                        (Tab::Profiles, "Profiles"),
                    ]);
                },
                |ui| {
                    if connected { w::status_dot(ui, GOOD, "Connected") } else { w::status_dot(ui, DIM, "Not connected") }
                    divider(ui);
                    ui.label(pname);
                    w::small_caps(ui, "Editing", MUTED);
                },
            );
            self.tab = tab;
            body(ui, |ui| match self.tab {
                Tab::Performance => self.performance(ui),
                Tab::Lighting => self.lighting(ui),
                Tab::Buttons => self.buttons(ui),
                Tab::Profiles => self.profiles(ui),
            });
        });

        self.dialogs(ctx);
        if serde_json::to_string(&self.s).unwrap_or_default() != before {
            self.s.save();
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_reset {
            modal(ctx, "Reset profile", |ui| {
                ui.label(format!("Reset “{}” to factory defaults?", self.s.profiles[self.profile].name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if w::primary_button(ui, "Reset", true).clicked() {
                        self.s.profiles[self.profile] = polaris::Profile { name: format!("Profile {}", self.profile + 1), ..Default::default() };
                        self.confirm_reset = false;
                    }
                    if w::button(ui, "Cancel", true).clicked() {
                        self.confirm_reset = false;
                    }
                });
            });
        }
        if let Some((idx, picker)) = &mut self.picker {
            if let Some(res) = picker.show(ctx) {
                if let Some((m, u)) = res {
                    self.s.profiles[self.profile].buttons[*idx] = Some([0, m, u, 0]);
                }
                self.picker = None;
            }
        }
    }

    // ------------------------------------------------------------ performance
    fn performance(&mut self, ui: &mut egui::Ui) {
        let sync = &mut self.xy_sync;
        let p = &mut self.s.profiles[self.profile];
        ui.columns(2, |cols| {
            cols[0].spacing_mut().item_spacing.y = 24.0;
            card(&mut cols[0], "DPI stages", |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    caps(ui, "Stages", TEXT);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let opts: Vec<(u8, String)> = (1..=NUM_STAGES as u8).map(|n| (n, n.to_string())).collect();
                        let refs: Vec<(u8, &str)> = opts.iter().map(|(n, s)| (*n, s.as_str())).collect();
                        segmented(ui, &mut p.stage_count, &refs, true, 300.0);
                    });
                });
                if p.current_stage >= p.stage_count {
                    p.current_stage = p.stage_count - 1;
                }
                ui.add_space(8.0);
                for i in 0..NUM_STAGES {
                    let en = i < p.stage_count as usize;
                    ui.horizontal(|ui| {
                        let cur = p.current_stage == i as u8;
                        let resp = w::fixed_radio(ui, cur, &format!("{}", i + 1), en);
                        if resp.clicked() {
                            p.current_stage = i as u8;
                        }
                        let mut v = polaris::dpi_to_slider(p.dpi[i]);
                        let gap = ui.spacing().item_spacing.x;
                        let sw = (ui.available_width() - 76.0 - 30.0 - 3.0 * gap).max(80.0);
                        ui.allocate_ui(vec2(sw, 30.0), |ui| {
                            if slider(ui, &mut v, 1..=290, en).changed() {
                                p.dpi[i] = polaris::slider_to_dpi(v);
                            }
                        });
                        let mut dpi = p.dpi[i];
                        let r = ui.add_enabled_ui(en, |ui| {
                            ui.add_sized([76.0, 30.0], egui::DragValue::new(&mut dpi).range(polaris::DPI_MIN..=polaris::DPI_MAX).speed(50))
                        }).inner;
                        if r.changed() {
                            p.dpi[i] = polaris::slider_to_dpi(polaris::dpi_to_slider(dpi));
                        }
                        w::color_button(ui, &mut p.dpi_colors[i], en);
                    });
                }
                ui.add_space(6.0);
                hint(ui, "The dot marks the stage the mouse starts in. 50-DPI steps up to 10 000, 100-DPI steps up to 19 000 (PMW3370). Drag a number to fine-tune it.");
            });

            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = 24.0;
            card(ui, "Polling rate", |ui| {
                let opts: Vec<(u32, String)> = polaris::POLLING_RATES.iter().map(|r| (*r, format!("{r} Hz"))).collect();
                let refs: Vec<(u32, &str)> = opts.iter().map(|(n, s)| (*n, s.as_str())).collect();
                let wdt = ui.available_width();
                segmented(ui, &mut p.polling_rate, &refs, true, wdt);
            });
            card(ui, "Sensor", |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let (x0, y0) = (p.x_sensitivity, p.y_sensitivity);
                slider_row(ui, "X sensitivity", &mut p.x_sensitivity, 1..=100, "1", "100", Some(format!("{x0}%")), true);
                slider_row(ui, "Y sensitivity", &mut p.y_sensitivity, 1..=100, "1", "100", Some(format!("{y0}%")), true);
                toggle_row(ui, "Sync X and Y", sync, true);
                if *sync {
                    if p.x_sensitivity != x0 {
                        p.y_sensitivity = p.x_sensitivity;
                    } else if p.y_sensitivity != y0 {
                        p.x_sensitivity = p.y_sensitivity;
                    }
                }
                ui.add_space(6.0);
                toggle_row(ui, "Lift-off distance", &mut p.lod_enabled, true);
                ui.horizontal(|ui| {
                    let wdt = ui.available_width();
                    let lod = p.lod_enabled;
                    segmented(ui, &mut p.lod, &[(0, polaris::LOD_NAMES[0]), (1, polaris::LOD_NAMES[1])], lod, wdt);
                });
                hint(ui, "How high the mouse can be lifted before it stops tracking.");
                ui.add_space(6.0);
                let kr = format!("{} ms", p.key_response);
                slider_row(ui, "Key response", &mut p.key_response, 1..=100, "1", "100", Some(kr), true);
                hint(ui, "The vendor's Key response setting, stored per profile.");
            });
        });
    }

    // ------------------------------------------------------------ lighting
    fn lighting(&mut self, ui: &mut egui::Ui) {
        let p = &mut self.s.profiles[self.profile];
        let l = &mut self.s.lighting;
        ui.columns(2, |cols| {
            card(&mut cols[0], "Effect", |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.columns(2, |c| {
                    for (i, name) in polaris::LIGHT_MODES.iter().enumerate() {
                        let col = &mut c[i / 6];
                        if radio_item(col, p.light_mode == i as u8, name, true).clicked() {
                            p.light_mode = i as u8;
                        }
                    }
                });
                ui.add_space(10.0);
                hint(ui, "The effect is stored per profile.");
            });
            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = 24.0;
            let on = p.light_mode != 0;
            card(ui, "Parameters", |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let pct = format!("{}%", (p.light_brightness as u32 * 100 + 127) / 255);
                slider_row(ui, "Brightness", &mut p.light_brightness, 0..=255, "0", "255", Some(pct), on);
                ui.add_space(6.0);
                caps(ui, "Speed", if on && p.light_mode != 1 { TEXT } else { DIM });
                let wdt = ui.available_width();
                segmented(ui, &mut p.light_speed, &[(0, "Slow"), (1, "Medium"), (2, "Fast")], on && p.light_mode != 1, wdt);
            });
            let mode = p.light_mode;
            let pal = polaris::mode_uses_palette(mode);
            let single = polaris::mode_uses_single_color(mode);
            card(ui, "Colours", |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                let wdt = ui.available_width();
                segmented(ui, &mut l.cycle, &[(false, "Fixed colour"), (true, "Colour cycle")], pal, wdt);
                ui.horizontal(|ui| {
                    if l.cycle {
                        for c in l.cycle_colors.iter_mut() {
                            w::color_button(ui, c, pal);
                        }
                    } else {
                        w::color_button(ui, &mut l.fixed_color, pal);
                        w::muted(ui, "Used by Standard, Wave and Finger movement");
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    caps(ui, "Slide colour", if single { TEXT } else { DIM });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        w::color_button(ui, &mut l.single_color, single);
                    });
                });
                if !pal && !single {
                    hint(ui, "This effect uses the mouse's built-in colours.");
                } else {
                    hint(ui, "Colours are shared by all profiles.");
                }
            });
        });
    }

    // ------------------------------------------------------------ buttons
    fn buttons(&mut self, ui: &mut egui::Ui) {
        let profile = self.profile;
        let defaults = polaris::default_buttons(profile);
        ui.columns(2, |cols| {
            card(&mut cols[0], "Buttons", |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for i in 0..NUM_USER_BUTTONS {
                    let cur = self.s.profiles[profile].buttons[i];
                    let raw = cur.unwrap_or(defaults[i].0);
                    let sel = self.button == i;
                    let r = w::two_line_item(ui, sel, polaris::BUTTON_NAMES[i], &format!(
                        "{}{}", polaris::describe_button(raw), if cur.is_none() { "  · default" } else { "" }));
                    if r.clicked() {
                        self.button = i;
                    }
                }
                ui.add_space(8.0);
                hint(ui, "Pick a button, then choose what it does. The scroll wheel keeps its factory assignment.");
            });
            let ui = &mut cols[1];
            let i = self.button;
            let cur = self.s.profiles[profile].buttons[i];
            let raw = cur.unwrap_or(defaults[i].0);
            card(ui, &format!("{} function", polaris::BUTTON_NAMES[i].split(" (").next().unwrap_or("")), |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let mut groups: Vec<&str> = vec![];
                for f in polaris::BUTTON_FUNCTIONS {
                    if !groups.contains(&f.group) {
                        groups.push(f.group);
                    }
                }
                egui::ScrollArea::vertical().max_height(470.0).auto_shrink([false, true]).show(ui, |ui| {
                    for g in groups {
                        ui.add_space(8.0);
                        w::small_caps(ui, g, MUTED);
                        ui.add_space(2.0);
                        for f in polaris::BUTTON_FUNCTIONS.iter().filter(|f| f.group == g) {
                            if radio_item(ui, raw == f.raw, f.label, true).clicked() {
                                self.s.profiles[profile].buttons[i] = Some(f.raw);
                            }
                        }
                    }
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if w::button(ui, "Keyboard key…", true).clicked() {
                        self.picker = Some((i, ComboPicker::new(format!("{}: keyboard key", polaris::BUTTON_NAMES[i]))));
                    }
                    if w::button(ui, "Factory default", cur.is_some()).clicked() {
                        self.s.profiles[profile].buttons[i] = None;
                    }
                });
            });
        });
    }

    // ------------------------------------------------------------ profiles
    fn profiles(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |cols| {
            card(&mut cols[0], "Profiles", |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for i in 0..NUM_PROFILES {
                    let name = self.s.profiles[i].name.clone();
                    let sub = if i == self.s.active_profile { "Active on the mouse" } else { "Stored on the mouse" };
                    if w::two_line_item(ui, self.profile == i, &format!("{}  ·  {}", i + 1, name), sub).clicked() {
                        self.profile = i;
                    }
                }
                ui.add_space(8.0);
                hint(ui, "The mouse keeps five profiles. Pick the one to edit here; the other tabs edit it.");
            });
            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = 24.0;
            let active = self.s.active_profile;
            card(ui, "This profile", |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                w::small_caps(ui, "Name", MUTED);
                ui.add(egui::TextEdit::singleline(&mut self.s.profiles[self.profile].name).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
                ui.add_space(6.0);
                let p = &self.s.profiles[self.profile];
                kv(ui, "Polling rate", &format!("{} Hz", p.polling_rate));
                kv(ui, "DPI stages", &p.dpi[..p.stage_count as usize].iter().map(|d| d.to_string()).collect::<Vec<_>>().join(" · "));
                kv(ui, "Lighting", polaris::LIGHT_MODES[p.light_mode as usize]);
                ui.add_space(8.0);
                let can = self.connected && !self.job.is_some() && active != self.profile;
                ui.horizontal(|ui| {
                    if w::button(ui, "Make active on mouse", can).clicked() {
                        self.s.active_profile = self.profile;
                        let p = self.profile;
                        self.job = Some(Job::spawn(ui.ctx(), move |_| polaris::Device::open()?.switch_profile(p)));
                    }
                    if active == self.profile {
                        w::pill_badge(ui, "Active", GOOD, GOOD_SOFT);
                    }
                });
            });
            card(ui, "About", |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(concat!("PolariTha ", env!("CARGO_PKG_VERSION")));
                hint(ui, "An unofficial Linux configurator for the AQIRYS Polaris Wired and Talitha, not affiliated with AQIRYS.");
                hint(ui, "The vendor tool never reads the mouse, so settings live on this PC and Apply writes all profiles.");
            });
        });
    }
}

/// Centered dialog window.
pub fn modal(ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(egui::Frame::new().fill(POPUP).stroke(egui::Stroke::new(1.0_f32, LINE)).corner_radius(egui::CornerRadius::same(RADIUS)).inner_margin(egui::Margin::same(24)))
        .show(ctx, |ui| {
            ui.add(egui::Label::new(w::spaced(&title.to_uppercase(), TITLE_SIZE, TEXT, false, 1.0)));
            ui.add_space(8.0);
            add(ui);
        });
}
