//! AQIRYS Polaris Wired (04d9:a2e7, PMW3370) protocol and settings model.
//!
//! Reverse engineered from the vendor MouseDriver.exe / hiddll.dll:
//! * commands are 8-byte feature reports (report id 0) on the vendor interface
//!   (usage page 0xFF00); byte 7 = 0xFF - sum(bytes 0..6)
//! * 128-byte blocks follow some commands as two 64-byte output reports
//! * the vendor tool never reads the mouse back; state lives on disk.

use crate::hidraw::{self, Hidraw};
use serde::{Deserialize, Serialize};
use std::io;
use std::thread::sleep;
use std::time::Duration;

pub const VID: u16 = 0x04D9;
pub const PID: u16 = 0xA2E7;
pub const USAGE_PAGE: u32 = 0xFF00;
pub const NUM_PROFILES: usize = 5;
pub const NUM_STAGES: usize = 8;
pub const NUM_USER_BUTTONS: usize = 9;

fn ini_block(lines: [&str; 8]) -> [u8; 128] {
    let mut out = [0u8; 128];
    let mut i = 0;
    for l in lines {
        for x in l.split(',') {
            out[i] = u8::from_str_radix(x, 16).unwrap();
            i += 1;
        }
    }
    out
}

/// Data.ini [ATTRIBUTE_0] (global block template)
fn attr_global_template() -> [u8; 128] {
    ini_block([
        "88,00,3f,00,42,03,fe,0f,00,00,00,00,00,00,00,00",
        "00,01,02,03,05,04,04,06,ff,00,00,00,ff,00,00,00",
        "ff,ff,ff,00,00,ff,ff,ff,00,ff,ff,ff,ff,ff,ff,ff",
        "ff,00,00,00,ff,00,00,00,ff,ff,ff,00,00,00,00,00",
        "8f,04,0a,0a,19,19,06,05,ff,04,64,64,01,c0,f0,03",
        "64,00,20,20,14,28,3c,50,a0,40,61,77,14,28,3c,50",
        "a0,40,61,77,ff,ff,ff,ff,ff,00,00,00,00,ff,00,ff",
        "00,ff,ff,00,00,ff,ff,ff,00,ff,ff,55,00,ff,ff,ff",
    ])
}

/// Data.ini [ATTRIBUTE_1] (per-profile block template)
fn attr_profile_template() -> [u8; 128] {
    ini_block([
        "ff,ff,00,00,00,00,00,00,00,00,00,00,00,00,00,00",
        "00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00",
        "00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00",
        "2e,10,42,00,00,00,00,00,00,00,00,00,00,00,00,00",
        "8f,04,0a,0a,19,19,06,05,ff,04,64,64,01,c0,f0,03",
        "64,00,20,20,14,28,3c,50,a0,40,61,77,14,28,3c,50",
        "a0,40,61,77,ff,ff,ff,ff,ff,00,00,00,00,ff,00,ff",
        "00,ff,ff,00,00,ff,ff,ff,00,ff,ff,55,00,ff,ff,ff",
    ])
}

/// Data.ini [BUTTON] defaults for one profile: (wire bytes, user configurable)
pub fn default_buttons(profile: usize) -> [([u8; 4], bool); 16] {
    let mut b = [([0u8; 4], false); 16];
    let common: [[u8; 4]; 9] = [
        [1, 0, 0xF0, 0], [1, 0, 0xF1, 0], [1, 0, 0xF2, 0], [1, 0, 0xF4, 0], [1, 0, 0xF3, 0],
        [7, 0, 1, 0], [0x0B, 9, 0, 0], [0x0C, 0, 0, 0], [0x0C, 0, 0, 0],
    ];
    for (i, c) in common.iter().enumerate() {
        b[i] = (*c, true);
    }
    // wheel entries; profile 2 has them swapped in the vendor file
    let (a, z) = if profile == 1 { ([4, 0, 2, 0], [4, 0, 1, 0]) } else { ([4, 0, 1, 0], [4, 0, 2, 0]) };
    b[14] = (a, false);
    b[15] = (z, false);
    b
}

pub const BUTTON_NAMES: [&str; NUM_USER_BUTTONS] = [
    "Button 1 (Left)", "Button 2 (Right)", "Button 3 (Middle)", "Button 4 (Forward)",
    "Button 5 (Backward)", "Button 6 (DPI)", "Button 7", "Button 8", "Button 9",
];

pub const POLLING_RATES: [u32; 4] = [125, 250, 500, 1000];
fn rate_code(rate: u32) -> u8 {
    match rate {
        125 => 8,
        250 => 4,
        500 => 2,
        _ => 1,
    }
}

pub const LIGHT_MODES: [&str; 12] = [
    "Off", "Standard", "Twinkle", "Breathing", "Neon", "Wave", "Slide",
    "Finger movement", "Flip up and down", "YO-YO", "Cross flash", "Flying star",
];
pub fn mode_uses_single_color(m: u8) -> bool {
    m == 6
}
pub fn mode_uses_palette(m: u8) -> bool {
    matches!(m, 1 | 5 | 7)
}
pub const LOD_NAMES: [&str; 2] = ["Low", "High"];
const LOD_VALUES: [u8; 2] = [1, 2];
const DPI_COLOR_ORDER: [u8; 8] = [0, 2, 1, 3, 4, 5, 6, 7];

/// Button functions (vendor function id order), grouped for menus.
pub struct ButtonFn {
    pub label: &'static str,
    pub group: &'static str,
    pub raw: [u8; 4],
}

pub const BUTTON_FUNCTIONS: &[ButtonFn] = &[
    ButtonFn { label: "Left click", group: "Mouse", raw: [1, 0, 0xF0, 0] },
    ButtonFn { label: "Right click", group: "Mouse", raw: [1, 0, 0xF1, 0] },
    ButtonFn { label: "Middle click", group: "Mouse", raw: [1, 0, 0xF2, 0] },
    ButtonFn { label: "Forward", group: "Mouse", raw: [1, 0, 0xF4, 0] },
    ButtonFn { label: "Backward", group: "Mouse", raw: [1, 0, 0xF3, 0] },
    ButtonFn { label: "Scroll up", group: "Mouse", raw: [4, 0, 1, 0] },
    ButtonFn { label: "Scroll down", group: "Mouse", raw: [4, 0, 2, 0] },
    ButtonFn { label: "Double click", group: "Mouse", raw: [0x0A, 0xF0, 0x32, 2] },
    ButtonFn { label: "Triple click", group: "Mouse", raw: [0x0A, 0xF0, 0x32, 3] },
    ButtonFn { label: "Sniper", group: "Mouse", raw: [0x0B, 0, 0, 0] },
    ButtonFn { label: "DPI cycle", group: "DPI / profile", raw: [7, 0, 3, 0] },
    ButtonFn { label: "DPI +", group: "DPI / profile", raw: [7, 0, 1, 0] },
    ButtonFn { label: "DPI -", group: "DPI / profile", raw: [7, 0, 2, 0] },
    ButtonFn { label: "Profile cycle", group: "DPI / profile", raw: [8, 0, 3, 0] },
    ButtonFn { label: "Profile +", group: "DPI / profile", raw: [8, 0, 1, 0] },
    ButtonFn { label: "Profile -", group: "DPI / profile", raw: [8, 0, 2, 0] },
    ButtonFn { label: "Polling rate cycle", group: "DPI / profile", raw: [5, 0, 3, 0] },
    ButtonFn { label: "Polling rate +", group: "DPI / profile", raw: [5, 0, 1, 0] },
    ButtonFn { label: "Polling rate -", group: "DPI / profile", raw: [5, 0, 2, 0] },
    ButtonFn { label: "Light mode switch", group: "DPI / profile", raw: [0x0C, 0, 0, 0] },
    ButtonFn { label: "Media player", group: "Media", raw: [3, 0, 0x83, 0x01] },
    ButtonFn { label: "Play / pause", group: "Media", raw: [3, 0, 0xCD, 0] },
    ButtonFn { label: "Stop", group: "Media", raw: [3, 0, 0xB7, 0] },
    ButtonFn { label: "Previous track", group: "Media", raw: [3, 0, 0xB6, 0] },
    ButtonFn { label: "Next track", group: "Media", raw: [3, 0, 0xB5, 0] },
    ButtonFn { label: "Volume up", group: "Media", raw: [3, 0, 0xE9, 0] },
    ButtonFn { label: "Volume down", group: "Media", raw: [3, 0, 0xEA, 0] },
    ButtonFn { label: "Mute", group: "Media", raw: [3, 0, 0xE2, 0] },
    ButtonFn { label: "Browser home", group: "Applications", raw: [3, 0, 0x23, 0x02] },
    ButtonFn { label: "Browser favorites", group: "Applications", raw: [3, 0, 0x2A, 0x02] },
    ButtonFn { label: "Browser stop", group: "Applications", raw: [3, 0, 0x26, 0x02] },
    ButtonFn { label: "Browser refresh", group: "Applications", raw: [3, 0, 0x27, 0x02] },
    ButtonFn { label: "Browser search", group: "Applications", raw: [3, 0, 0x21, 0x02] },
    ButtonFn { label: "E-mail", group: "Applications", raw: [3, 0, 0x8A, 0x01] },
    ButtonFn { label: "Calculator", group: "Applications", raw: [3, 0, 0x92, 0x01] },
    ButtonFn { label: "My computer", group: "Applications", raw: [3, 0, 0x94, 0x01] },
    ButtonFn { label: "Cut (Ctrl+X)", group: "Shortcuts", raw: [0, 1, 0x1B, 0] },
    ButtonFn { label: "Copy (Ctrl+C)", group: "Shortcuts", raw: [0, 1, 0x06, 0] },
    ButtonFn { label: "Paste (Ctrl+V)", group: "Shortcuts", raw: [0, 1, 0x19, 0] },
    ButtonFn { label: "Undo (Ctrl+Z)", group: "Shortcuts", raw: [0, 1, 0x1D, 0] },
    ButtonFn { label: "Redo (Ctrl+Y)", group: "Shortcuts", raw: [0, 1, 0x1C, 0] },
    ButtonFn { label: "Select all (Ctrl+A)", group: "Shortcuts", raw: [0, 1, 0x04, 0] },
    ButtonFn { label: "Find (Ctrl+F)", group: "Shortcuts", raw: [0, 1, 0x09, 0] },
    ButtonFn { label: "Save (Ctrl+S)", group: "Shortcuts", raw: [0, 1, 0x16, 0] },
    ButtonFn { label: "Open (Ctrl+O)", group: "Shortcuts", raw: [0, 1, 0x12, 0] },
    ButtonFn { label: "New (Ctrl+N)", group: "Shortcuts", raw: [0, 1, 0x11, 0] },
    ButtonFn { label: "Print (Ctrl+P)", group: "Shortcuts", raw: [0, 1, 0x13, 0] },
    ButtonFn { label: "Close window (Alt+F4)", group: "Shortcuts", raw: [0, 4, 0x3D, 0] },
    ButtonFn { label: "Switch window (Alt+Tab)", group: "Shortcuts", raw: [0, 4, 0x2B, 0] },
    ButtonFn { label: "Show desktop (Super+D)", group: "Shortcuts", raw: [0, 8, 0x07, 0] },
    ButtonFn { label: "Run (Super+R)", group: "Shortcuts", raw: [0, 8, 0x15, 0] },
    ButtonFn { label: "Lock screen (Super+L)", group: "Shortcuts", raw: [0, 8, 0x0F, 0] },
    ButtonFn { label: "Disabled", group: "Other", raw: [0, 0, 0, 0] },
];

pub fn describe_button(raw: [u8; 4]) -> String {
    if let Some(f) = BUTTON_FUNCTIONS.iter().find(|f| f.raw == raw) {
        return f.label.to_string();
    }
    if raw[0] == 0 && raw[3] == 0 && raw[2] != 0 {
        return crate::keycodes::describe_combo(raw[1], raw[2]);
    }
    format!("Factory function {:02X} {:02X}", raw[0], raw[1])
}

// ---- DPI encoding for sensor 3370: slider 1..290, 50-DPI steps to 10000, then 100.
pub const DPI_MIN: u32 = 50;
pub const DPI_MAX: u32 = 19000;

pub fn dpi_to_slider(dpi: u32) -> u32 {
    let d = dpi.clamp(DPI_MIN, DPI_MAX);
    if d <= 10000 {
        ((d + 25) / 50).max(1)
    } else {
        200 + (d - 10000 + 50) / 100
    }
}
pub fn slider_to_dpi(v: u32) -> u32 {
    if v <= 200 { v * 50 } else { 10000 + (v - 200) * 100 }
}
fn dpi_wire(v: u32) -> (u8, bool) {
    if v > 200 { (((v + 0x9B) & 0xFF) as u8, true) } else { (((v - 1) & 0xFF) as u8, false) }
}

// ---- settings --------------------------------------------------------------

pub type Rgb = [u8; 3];

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub polling_rate: u32,
    pub dpi: [u32; NUM_STAGES],
    pub dpi_colors: [Rgb; NUM_STAGES],
    pub stage_count: u8,
    pub current_stage: u8,
    pub x_sensitivity: u8,
    pub y_sensitivity: u8,
    pub lod_enabled: bool,
    pub lod: u8,
    pub key_response: u8,
    pub light_mode: u8,
    pub light_brightness: u8,
    pub light_speed: u8,
    /// None = factory default
    pub buttons: [Option<[u8; 4]>; NUM_USER_BUTTONS],
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Profile".into(),
            polling_rate: 1000,
            dpi: [400, 800, 1600, 3200, 8000, 10000, 19000, 4000],
            dpi_colors: [
                [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0],
                [255, 0, 255], [255, 0x53, 0], [255, 255, 255], [255, 255, 255],
            ],
            stage_count: 7,
            current_stage: 2,
            x_sensitivity: 100,
            y_sensitivity: 100,
            lod_enabled: true,
            lod: 0,
            key_response: 20,
            light_mode: 2,
            light_brightness: 255,
            light_speed: 2,
            buttons: [None; NUM_USER_BUTTONS],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Lighting {
    pub cycle: bool,
    pub fixed_color: Rgb,
    pub single_color: Rgb,
    pub cycle_colors: [Rgb; 8],
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            cycle: true,
            fixed_color: [255, 0, 0],
            single_color: [0, 0, 255],
            cycle_colors: [
                [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0],
                [255, 0, 255], [0, 255, 255], [255, 255, 255], [255, 255, 255],
            ],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub active_profile: usize,
    pub lighting: Lighting,
    pub profiles: [Profile; NUM_PROFILES],
}

impl Default for Settings {
    fn default() -> Self {
        let profiles = std::array::from_fn(|i| Profile { name: format!("Profile {}", i + 1), ..Default::default() });
        Self { active_profile: 0, lighting: Lighting::default(), profiles }
    }
}

impl Settings {
    pub fn load() -> Self {
        crate::config::load("polaris.json")
    }
    pub fn save(&self) {
        crate::config::save("polaris.json", self);
    }
}

pub fn build_profile_attributes(p: &Profile) -> [u8; 128] {
    let mut a = attr_profile_template();
    if p.lod_enabled { a[0x41] |= 1 } else { a[0x41] &= 0xFE }
    a[0x46] = p.stage_count.clamp(1, NUM_STAGES as u8);
    a[0x47] = p.light_mode.min(LIGHT_MODES.len() as u8 - 1);
    a[0x48] = p.light_brightness;
    a[0x49] = p.light_speed.min(2);
    a[0x4A] = p.x_sensitivity.clamp(1, 100);
    a[0x4B] = p.y_sensitivity.clamp(1, 100);
    let mut hi = 0u8;
    for (i, &d) in p.dpi.iter().enumerate() {
        let (b, dbl) = dpi_wire(dpi_to_slider(d));
        a[0x54 + i] = b;
        a[0x5C + i] = b;
        if dbl {
            hi |= 1 << i;
        }
    }
    a[0x52] = hi;
    a[0x53] = hi;
    if p.lod_enabled {
        a[0x66] = LOD_VALUES[(p.lod as usize).min(1)];
    }
    a[0x67] = p.key_response.clamp(1, 100);
    for (i, c) in p.dpi_colors.iter().enumerate() {
        a[0x68 + 3 * i..0x6B + 3 * i].copy_from_slice(c);
    }
    a
}

pub fn build_global_attributes(s: &Settings) -> [u8; 128] {
    let mut a = attr_global_template();
    a[0x10..0x18].copy_from_slice(&DPI_COLOR_ORDER);
    let mode = s.profiles[s.active_profile].light_mode;
    if mode_uses_single_color(mode) {
        a[0x18..0x1B].copy_from_slice(&s.lighting.single_color);
    } else if mode_uses_palette(mode) {
        for i in 0..8 {
            let c = if s.lighting.cycle { s.lighting.cycle_colors[i] } else { s.lighting.fixed_color };
            a[0x18 + 3 * i..0x1B + 3 * i].copy_from_slice(&c);
        }
    }
    a[0x30] = 0xFF;
    a[0x32] = 0;
    a[0x33..0x3C].fill(0);
    a
}

pub fn build_buttons(s: &Settings, profile: usize) -> [u8; 128] {
    let mut out = [0u8; 128];
    for (i, (raw, configurable)) in default_buttons(profile).iter().enumerate() {
        let mut b = *raw;
        if *configurable && i < NUM_USER_BUTTONS {
            if let Some(u) = s.profiles[profile].buttons[i] {
                b = u;
            }
        }
        out[4 * i..4 * i + 4].copy_from_slice(&b);
    }
    out
}

// ---- transport -------------------------------------------------------------

pub fn find_device() -> Option<hidraw::HidrawInfo> {
    hidraw::find(VID, PID, USAGE_PAGE)
}

pub struct Device {
    dev: Hidraw,
    pub verbose: bool,
}

impl Device {
    pub fn open() -> io::Result<Self> {
        let info = find_device().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "AQIRYS Polaris Wired not found (04d9:a2e7)"))?;
        Ok(Self { dev: Hidraw::open(&info.node)?, verbose: false })
    }

    fn feature(&self, payload: &[u8]) -> io::Result<()> {
        let mut body = [0u8; 7];
        body[..payload.len()].copy_from_slice(payload);
        let chk = 0xFFu8.wrapping_sub(body.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
        let mut buf = vec![0u8];
        buf.extend_from_slice(&body);
        buf.push(chk);
        if self.verbose {
            eprintln!("  FEAT {:02x?}", &buf[1..]);
        }
        self.dev.set_feature(&buf)?;
        sleep(Duration::from_millis(20));
        Ok(())
    }

    fn bulk(&self, data: &[u8; 128]) -> io::Result<()> {
        for half in data.chunks(64) {
            let mut buf = vec![0u8];
            buf.extend_from_slice(half);
            if self.verbose {
                eprintln!("  OUT  {:02x?}", half);
            }
            self.dev.write(&buf)?;
            sleep(Duration::from_millis(30));
        }
        Ok(())
    }

    pub fn begin(&self) -> io::Result<()> {
        self.feature(&[0x01, 0x88])
    }
    pub fn select_profile(&self, p: usize) -> io::Result<()> {
        self.feature(&[0x02, p as u8 + 1])
    }
    fn write_block(&self, cmd: u8, slot: u8, data: &[u8; 128]) -> io::Result<()> {
        self.feature(&[cmd, slot, 0x80])?;
        sleep(Duration::from_millis(30));
        self.bulk(data)
    }

    /// The vendor "Apply" sequence (FUN_00415ac0).
    pub fn apply(&self, s: &Settings, progress: &dyn Fn(usize, usize)) -> io::Result<()> {
        let total = 3 + 2 * NUM_PROFILES;
        let mut done = 0;
        let mut step = || {
            done += 1;
            progress(done, total)
        };
        self.begin()?;
        self.select_profile(s.active_profile)?;
        step();
        for (i, p) in s.profiles.iter().enumerate() {
            let slot = i as u8 + 1;
            self.feature(&[0x03, slot, rate_code(p.polling_rate), 0])?;
            let cur = p.current_stage.min(p.stage_count.saturating_sub(1));
            self.feature(&[0x04, slot, cur + 1])?;
            self.write_block(0x0C, slot, &build_profile_attributes(p))?;
            step();
        }
        for i in 0..NUM_PROFILES {
            self.write_block(0x0D, i as u8 + 1, &build_buttons(s, i))?;
            step();
        }
        self.write_block(0x0C, 0, &build_global_attributes(s))?;
        step();
        self.begin()?;
        self.select_profile(s.active_profile)?;
        step();
        Ok(())
    }

    pub fn switch_profile(&self, p: usize) -> io::Result<()> {
        self.begin()?;
        self.select_profile(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn dpi_encoding() {
        assert_eq!(dpi_to_slider(400), 8);
        assert_eq!(dpi_wire(dpi_to_slider(10000)), (199, false));
        assert_eq!(dpi_wire(dpi_to_slider(10100)), (100, true));
        assert_eq!(dpi_wire(dpi_to_slider(19000)), (189, true));
        assert_eq!(slider_to_dpi(dpi_to_slider(12345)), 12300);
    }

    #[test]
    fn default_blocks() {
        let s = Settings::default();
        assert_eq!(hex(&build_profile_attributes(&s.profiles[0])),
            "ffff000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002e1042000000000000000000000000008f050a0a19190702ff02646401c0f00364004040070f1f3f9fc7bd4f070f1f3f9fc7bd4fffff0114ff000000ff000000ffffff00ff00ffff5300ffffffffffff");
        assert_eq!(hex(&build_global_attributes(&s)),
            "88003f004203fe0f00000000000000000002010304050607ff000000ff000000ffffff0000ffffff00ffffffffffffffff0000000000000000000000000000008f040a0a19190605ff04646401c0f0036400202014283c50a040617714283c50a0406177ffffffffff00000000ff00ff00ffff0000ffffff00ffff5500ffffff");
        assert_eq!(hex(&build_buttons(&s, 0)[..64]),
            "0100f0000100f1000100f2000100f4000100f300070001000b0900000c0000000c00000000000000000000000000000000000000000000000400010004000200");
    }
}
