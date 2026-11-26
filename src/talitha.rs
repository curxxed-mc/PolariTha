//! AQIRYS Talitha keyboard over USB cable (258a:0236), its 2.4 GHz receiver
//! (3554:fa09) or Bluetooth (3554:fa08).
//!
//! The wired path ("CDevG5KB") uses 520-byte feature reports, see `wired_*`.
//! The receiver and Bluetooth use the "CDev3632" packets described below.
//!
//! Reverse engineered from the vendor DeviceDriver.exe ("CDev3632" transport
//! and the shared "CDevG5KB::ApplySetting" logic):
//! * vendor interface: usage page 0xFF02, report id 0x13, 19-byte reports
//! * packet: [0x13, cmd, n_packets, index, (hi<<4)|len, 14 data bytes, sum]
//!   with sum = (sum of bytes 0..18) & 0xFF; each written packet is acked by a
//!   packet carrying the same cmd (bit 7 of byte 2 = CRC error)
//! * reads use cmd | 0x40 and return n_packets packets of up to 14 bytes
//!
//! Commands: 0x01 key matrix (hi = layer, 126 x 4 bytes), 0x02 per-key colours
//! (hi = group, 3 planes x 126), 0x04/0x44 128-byte config (ends 5A A5),
//! 0x06 factory reset, 0x07 online check, 0x09 per-effect palette (20 x 21).

use crate::hidraw::{self, Hidraw};
use crate::keycodes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const DONGLE_VID: u16 = 0x3554;
pub const DONGLE_PID: u16 = 0xFA09;
/// Keyboard connected directly over Bluetooth: 0xFA07 (BLE, matched by the
/// vendor tool as "pid&fa07") and 0xFA08 ("Talitha BT3.0", classic BT).
pub const BT_PIDS: [u16; 2] = [0xFA07, 0xFA08];
pub const USAGE_PAGE: u32 = 0xFF02;
/// Wired (USB cable) mode: Sinowealth-based board, vendor feature report 6.
pub const WIRED_VID: u16 = 0x258A;
pub const WIRED_PID: u16 = 0x0236;
pub const WIRED_USAGE_PAGE: u32 = 0xFF00;
const WIRED_REPORT_ID: u8 = 0x06;
const WIRED_REPORT_LEN: usize = 0x208; // 520 bytes including the report id
const WIRED_CHUNK: usize = 0x200;
/// Wired command numbers (CDevG5KB); they differ from the wireless ones.
const W_MATRIX: u8 = 0x03;
const W_LED: u8 = 0x04;
const W_CUSTOM: u8 = 0x06;
const W_PALETTE: u8 = 0x0A;
const W_RESET: u8 = 0x11;
const W_PSD: u8 = 0x02;
const W_READ: u8 = 0x80;
const REPORT_ID: u8 = 0x13;
const CHUNK: usize = 14;
pub const MATRIX_LEN: usize = 126;
/// KB.ini RGBIndex=1,0,2: R goes to slot 1, G to 0, B to 2.
const RGB_ORDER: [usize; 3] = [1, 0, 2];

const CMD_MATRIX: u8 = 0x01;
const CMD_CUSTOM: u8 = 0x02;
const CMD_LED: u8 = 0x04;
const CMD_RESET: u8 = 0x06;
const CMD_PSD: u8 = 0x05;
const CMD_ONLINE: u8 = 0x07;
/// KB.ini Psd for the Talitha (PID 0236).
pub const TALITHA_PSD: [u8; 6] = [0x03, 0x00, 0x00, 0x00, 0x03, 0x5E];
const CMD_PALETTE: u8 = 0x09;
const READ: u8 = 0x40;

pub struct Effect {
    pub name: &'static str,
    pub speed: bool,
    pub brightness: bool,
    pub colors: bool,
}
const fn fx(name: &'static str, speed: bool, brightness: bool, colors: bool) -> Effect {
    Effect { name, speed, brightness, colors }
}
/// Effect list for this keyboard (Fw=1 -> "tc_eft"), capabilities from KB.ini LedOpt.
pub const EFFECTS: [Effect; 19] = [
    fx("Fixed on", false, true, true),
    fx("Respire", true, true, true),
    fx("Rainbow", true, true, false),
    fx("Flash away", true, true, true),
    fx("Raindrops", true, true, true),
    fx("Rainbow wheel", true, true, true),
    fx("Ripples shining", true, true, true),
    fx("Stars twinkle", true, true, true),
    fx("Shadow disappear", true, true, true),
    fx("Retro snake", true, true, true),
    fx("Neon stream", true, true, true),
    fx("Reaction", true, true, true),
    fx("Sine wave", true, true, true),
    fx("Retinue scanning", true, true, true), // hidden by LedMask=0x2000
    fx("Rotating windmill", true, true, false),
    fx("Colorful waterfall", true, true, false),
    fx("Blossoming", true, true, false),
    fx("Self-define (per-key)", false, true, false),
    fx("Off", false, false, false),
];
pub const HIDDEN_EFFECT: usize = 13;
pub const CUSTOM_EFFECT: usize = 17;
pub const OFF_EFFECT: usize = 18;
pub const BRIGHTNESS_MAX: u8 = 4;
pub const SPEED_MIN: u8 = 1;
pub const SPEED_MAX: u8 = 4;
pub const RAINBOW: u8 = 7;
pub const DEFAULT_PALETTE: [[u8; 3]; 7] =
    [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [255, 0, 255], [0, 255, 255], [255, 255, 255]];
pub const SLEEP_OPTIONS: [(u8, &str); 5] =
    [(0x0A, "10 min"), (0x14, "20 min"), (0x28, "40 min"), (0x3C, "60 min"), (0x00, "Never")];

const CFG_CUSTOM_FLAG: usize = 0x09;
const CFG_EFFECT: usize = 0x0A;
const CFG_GROUP: usize = 0x0B;
const CFG_SLEEP: usize = 0x18;
const CFG_PARAMS: usize = 0x3A;
const CFG_CUSTOM_BRIGHTNESS: usize = 0x5E;
const CFG_MAGIC: usize = 0x7E;

// ---- layout ----------------------------------------------------------------

#[derive(Clone, Deserialize)]
pub struct Key {
    pub label: String,
    pub rect: [i32; 4],
    pub vk: u8,
    pub matrix: usize,
}

#[derive(Deserialize)]
struct LayoutFile {
    keys: Vec<Key>,
}

pub fn layout() -> Vec<Key> {
    let f: LayoutFile = serde_json::from_str(include_str!("../assets/talitha_layout.json")).expect("layout json");
    f.keys
}

pub type Code = [u8; 4];
pub const DISABLED: Code = [0; 4];

/// Factory layer-0 code (vendor keyinfo_to_hardware_code).
pub fn default_code(k: &Key) -> Code {
    match k.vk {
        0xFA => [0x0D, 0, 0, 0],       // Fn
        0x8F => [0x02, 0, 0x00, 0xE2], // knob press -> mute
        vk => [0x00, 0x00, keycodes::vk_to_hid(vk), 0x00],
    }
}
pub fn keyboard_code(mods: u8, usage: u8) -> Code {
    [0x00, mods, usage, 0x00]
}
pub fn consumer_code(u: u16) -> Code {
    [0x02, 0x00, (u >> 8) as u8, u as u8]
}
pub const MOUSE_BUTTONS: [(&str, u8); 5] =
    [("Left click", 1), ("Right click", 2), ("Middle click", 4), ("Back", 8), ("Forward", 16)];
pub fn mouse_code(mask: u8) -> Code {
    [0x31, mask, 0, 0]
}

pub fn describe_code(c: Code) -> String {
    if c == DISABLED {
        return "Disabled".into();
    }
    match c[0] {
        0x00 => keycodes::describe_combo(c[1], c[2]),
        0x02 => keycodes::consumer_name(((c[2] as u16) << 8) | c[3] as u16),
        0x0D => "Fn".into(),
        0x31 => MOUSE_BUTTONS
            .iter()
            .find(|(_, m)| *m == c[1])
            .map(|(n, _)| format!("Mouse {}", n.to_lowercase()))
            .unwrap_or_else(|| format!("{c:02X?}")),
        _ => format!("Raw {c:02X?}"),
    }
}

// ---- persisted state ---------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub palettes: Vec<[[u8; 3]; 7]>,
    /// LED (matrix) index -> colour
    pub custom_colors: BTreeMap<usize, [u8; 3]>,
    /// matrix index -> code
    pub keymap: BTreeMap<usize, Code>,
}

impl Default for State {
    fn default() -> Self {
        Self { palettes: vec![DEFAULT_PALETTE; EFFECTS.len()], custom_colors: BTreeMap::new(), keymap: BTreeMap::new() }
    }
}

impl State {
    pub fn load() -> Self {
        let mut s: State = crate::config::load("talitha.json");
        s.palettes.resize(EFFECTS.len(), DEFAULT_PALETTE);
        s
    }
    pub fn save(&self) {
        crate::config::save("talitha.json", self);
    }
}

// ---- 128-byte config ---------------------------------------------------------

#[derive(Clone)]
pub struct LedConfig(pub [u8; 128]);

impl LedConfig {
    pub fn valid(&self) -> bool {
        self.0[CFG_MAGIC] == 0x5A && self.0[CFG_MAGIC + 1] == 0xA5
    }
    pub fn effect(&self) -> usize {
        if self.0[CFG_CUSTOM_FLAG] != 0 {
            return CUSTOM_EFFECT;
        }
        match self.0[CFG_EFFECT] {
            0 => OFF_EFFECT,
            e => (e as usize - 1).min(EFFECTS.len() - 1),
        }
    }
    /// (brightness, speed, colour index 0..6 or 7 = rainbow)
    pub fn params(&self, e: usize) -> (u8, u8, u8) {
        if e == CUSTOM_EFFECT {
            return (self.0[CFG_CUSTOM_BRIGHTNESS], 0, 0);
        }
        let o = CFG_PARAMS + 2 * e;
        (self.0[o], self.0[o + 1] >> 4, self.0[o + 1] & 0x0F)
    }
    pub fn sleep(&self) -> u8 {
        self.0[CFG_SLEEP]
    }
    pub fn set_sleep(&mut self, v: u8) {
        self.0[CFG_SLEEP] = v;
    }
    /// Mirrors the vendor FillCfg for Fw != 2.
    pub fn set_effect(&mut self, e: usize, brightness: u8, speed: u8, color: u8) {
        let r = &mut self.0;
        r[CFG_CUSTOM_FLAG] = (e == CUSTOM_EFFECT) as u8;
        r[CFG_EFFECT] = if e == OFF_EFFECT { 0 } else { e as u8 + 1 };
        r[CFG_GROUP] = 0x20;
        r[0x38] = 0xFF;
        r[0x39] = 0xFF;
        let b = brightness.min(BRIGHTNESS_MAX);
        if e == CUSTOM_EFFECT {
            r[CFG_CUSTOM_BRIGHTNESS] = b;
        } else if e != OFF_EFFECT {
            let o = CFG_PARAMS + 2 * e;
            r[o] = b;
            r[o + 1] = (speed.clamp(SPEED_MIN, SPEED_MAX) << 4) | (color & 0x0F);
        }
    }
}

pub fn build_palette_table(palettes: &[[[u8; 3]; 7]]) -> Vec<u8> {
    let mut t = vec![0u8; 0x15 * (EFFECTS.len() + 1)];
    for (i, pal) in palettes.iter().take(EFFECTS.len()).enumerate() {
        let base = 0x15 + i * 0x15;
        for (k, c) in pal.iter().enumerate() {
            for comp in 0..3 {
                t[base + 3 * k + RGB_ORDER[comp]] = c[comp];
            }
        }
    }
    t
}

pub fn build_custom_planes(colors: &BTreeMap<usize, [u8; 3]>) -> Vec<u8> {
    let mut p = vec![0u8; 3 * MATRIX_LEN];
    for (&led, c) in colors {
        if led < MATRIX_LEN {
            for comp in 0..3 {
                p[RGB_ORDER[comp] * MATRIX_LEN + led] = c[comp];
            }
        }
    }
    p
}

pub fn build_matrix(keymap: &BTreeMap<usize, Code>, keys: &[Key]) -> Vec<u8> {
    let mut m = vec![0u8; 4 * MATRIX_LEN];
    for k in keys {
        let c = keymap.get(&k.matrix).copied().unwrap_or_else(|| default_code(k));
        m[4 * k.matrix..4 * k.matrix + 4].copy_from_slice(&c);
    }
    m
}

// ---- transport -----------------------------------------------------------------

pub fn find_dongle() -> Option<hidraw::HidrawInfo> {
    hidraw::find(DONGLE_VID, DONGLE_PID, USAGE_PAGE)
}

pub fn find_bluetooth() -> Option<hidraw::HidrawInfo> {
    BT_PIDS.iter().find_map(|&pid| hidraw::find(DONGLE_VID, pid, USAGE_PAGE))
}

/// The cable interface: the 0xFF00 collection that carries feature report 6.
pub fn find_wired() -> Option<hidraw::HidrawInfo> {
    hidraw::enumerate().into_iter().find(|i| {
        i.vid == WIRED_VID && i.pid == WIRED_PID && i.collections.contains(&WIRED_USAGE_PAGE) && i.report_ids.contains(&WIRED_REPORT_ID)
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Link {
    Wired,
    Receiver,
    Bluetooth,
}

impl Link {
    pub fn name(self) -> &'static str {
        match self {
            Link::Wired => "USB cable",
            Link::Receiver => "2.4 GHz receiver",
            Link::Bluetooth => "Bluetooth",
        }
    }
}

/// The node to configure through: the cable when plugged in (fast and
/// reliable), then Bluetooth (its node only exists while connected), then the
/// receiver.
pub fn find_device() -> Option<(hidraw::HidrawInfo, Link)> {
    find_wired()
        .map(|i| (i, Link::Wired))
        .or_else(|| find_bluetooth().map(|i| (i, Link::Bluetooth)))
        .or_else(|| find_dongle().map(|i| (i, Link::Receiver)))
}

fn packet(cmd: u8, n: usize, idx: usize, hi: u8, data: &[u8]) -> [u8; 20] {
    let mut p = [0u8; 20];
    p[0] = REPORT_ID;
    p[1] = cmd;
    p[2] = n as u8;
    p[3] = idx as u8;
    p[4] = (hi << 4) | (data.len() as u8 & 0x0F);
    p[5..5 + data.len()].copy_from_slice(data);
    p[19] = p[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    p
}

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.into())
}

pub struct Device {
    dev: Hidraw,
    pub link: Link,
    pub verbose: bool,
    /// Packets of the last config read that were filled from the cache.
    pub filled: std::cell::Cell<usize>,
}

const CONFIG_CACHE: &str = "talitha_config_cache.json";

fn load_config_cache() -> Option<LedConfig> {
    let v: Vec<u8> = crate::config::load(CONFIG_CACHE);
    let a: [u8; 128] = v.try_into().ok()?;
    let c = LedConfig(a);
    c.valid().then_some(c)
}

fn save_config_cache(c: &LedConfig) {
    crate::config::save(CONFIG_CACHE, &c.0.to_vec());
}

impl Device {
    pub fn open() -> io::Result<Self> {
        let (info, link) = find_device().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "AQIRYS Talitha not found (no USB cable, receiver or Bluetooth connection)")
        })?;
        Ok(Self { dev: Hidraw::open(&info.node)?, link, verbose: false, filled: std::cell::Cell::new(0) })
    }

    // ---- wired (CDevG5KB): 520-byte feature reports ----
    //
    // header: [report id, cmd, arg, 0, n_packets, index, len_lo, len_hi], then
    // up to 512 data bytes. A write is a single SetFeature per packet. A read
    // sends the header with cmd | 0x80, waits, then reads the feature report
    // back; byte 1 echoes the command and the data starts at byte 8.

    fn wired_header(cmd: u8, arg: u8, n: usize, idx: usize, len: usize) -> Vec<u8> {
        let mut b = vec![0u8; WIRED_REPORT_LEN];
        b[..8].copy_from_slice(&[WIRED_REPORT_ID, cmd, arg, 0, n as u8, idx as u8, len as u8, (len >> 8) as u8]);
        b
    }

    fn wired_set(&self, buf: &[u8]) -> io::Result<()> {
        if self.verbose {
            eprintln!("  SET {:02x?} …", &buf[..24]);
        }
        let mut last = None;
        for _ in 0..3 {
            match self.dev.set_feature(buf) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last = Some(e);
                    sleep(Duration::from_millis(50));
                }
            }
        }
        Err(last.unwrap())
    }

    fn wired_write(&self, cmd: u8, arg: u8, data: &[u8], progress: &dyn Fn(usize, usize)) -> io::Result<()> {
        let n = data.len().div_ceil(WIRED_CHUNK).max(1);
        for idx in 0..n {
            let chunk = &data[(idx * WIRED_CHUNK).min(data.len())..((idx + 1) * WIRED_CHUNK).min(data.len())];
            let mut buf = Self::wired_header(cmd, arg, n, idx, chunk.len());
            buf[8..8 + chunk.len()].copy_from_slice(chunk);
            sleep(Duration::from_millis(25));
            self.wired_set(&buf)?;
            progress(idx + 1, n);
        }
        Ok(())
    }

    fn wired_read(&self, cmd: u8, arg: u8, len: usize) -> io::Result<Vec<u8>> {
        let read_cmd = cmd | W_READ;
        let n = len.div_ceil(WIRED_CHUNK).max(1);
        let mut out = Vec::with_capacity(len);
        for idx in 0..n {
            let size = (len - idx * WIRED_CHUNK).min(WIRED_CHUNK);
            self.wired_set(&Self::wired_header(read_cmd, arg, n, idx, size))?;
            sleep(Duration::from_millis(25));
            let reply = self.dev.get_feature(WIRED_REPORT_ID, WIRED_REPORT_LEN)?;
            if self.verbose {
                eprintln!("  GET {:02x?} …", &reply[..24]);
            }
            if reply[1] != read_cmd {
                return Err(err(format!("keyboard answered 0x{:02X} to read command 0x{read_cmd:02X}", reply[1])));
            }
            out.extend_from_slice(&reply[8..8 + size]);
        }
        Ok(out)
    }

    // ---- receiver / Bluetooth (CDev3632) ----

    fn send(&self, p: &[u8; 20]) -> io::Result<()> {
        if self.verbose {
            eprintln!("  TX {:02x?}", p);
        }
        self.dev.write(p)
    }

    fn recv(&self, cmd: u8, timeout: Duration) -> io::Result<Option<[u8; 20]>> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let Some(p) = self.dev.read(left)? else { return Ok(None) };
            if p.len() < 20 || p[0] != REPORT_ID {
                continue; // key/consumer/mouse reports share this interface
            }
            if self.verbose {
                eprintln!("  RX {:02x?}", &p[..20]);
            }
            if p[1] == 0x0A {
                continue; // unsolicited notification
            }
            if p[1] & 0x7F == cmd & 0x7F {
                let mut out = [0u8; 20];
                out.copy_from_slice(&p[..20]);
                return Ok(Some(out));
            }
        }
    }

    pub fn get(&self, cmd: u8, len: usize, timeout: Duration, retries: usize) -> io::Result<Vec<u8>> {
        self.request(cmd, 0, len, timeout, retries)
    }

    /// `arg` goes in byte 4, as the vendor SendCMD_3632 does (e.g. GetPSD sends 6).
    ///
    /// Multi-packet replies can lose packets. Over classic Bluetooth the link
    /// sits in sniff mode: replies come in bursts roughly every 0.8 s and drop
    /// packets inside a burst. Every request makes the keyboard re-send the
    /// whole reply, so we keep re-requesting and merge packets by index until
    /// all `n_packets` have arrived or the overall deadline (timeout x retries)
    /// passes. On the receiver a re-request only happens after `timeout` of
    /// silence; on Bluetooth every 250 ms.
    pub fn request(&self, cmd: u8, arg: u8, len: usize, timeout: Duration, retries: usize) -> io::Result<Vec<u8>> {
        let (chunks, total) = self.collect(cmd, arg, timeout, retries)?;
        match total {
            Some(t) if chunks.len() >= t => {
                let mut out: Vec<u8> = chunks.values().flatten().copied().collect();
                out.resize(len, 0);
                Ok(out)
            }
            Some(t) => Err(err(format!("incomplete reply from keyboard for command 0x{cmd:02X} ({}/{t} packets)", chunks.len()))),
            None => Err(err(format!("no reply from keyboard for command 0x{cmd:02X}"))),
        }
    }

    /// Packets received for a read request, keyed by index, plus the packet count.
    fn collect(&self, cmd: u8, arg: u8, timeout: Duration, retries: usize) -> io::Result<(BTreeMap<usize, Vec<u8>>, Option<usize>)> {
        let mut chunks: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        let mut total: Option<usize> = None;
        let (resend_every, deadline) = match self.link {
            Link::Receiver | Link::Wired => (timeout, Instant::now() + timeout * retries as u32),
            Link::Bluetooth => (Duration::from_millis(250), Instant::now() + (timeout * retries as u32).max(Duration::from_secs(4))),
        };
        let mut req = packet(cmd, 1, 0, 0, &[]);
        req[4] = arg;
        req[19] = req[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        self.dev.drain();
        let mut last_send = Instant::now();
        self.send(&req)?;
        while Instant::now() < deadline {
            let wait = resend_every.saturating_sub(last_send.elapsed()).min(deadline.saturating_duration_since(Instant::now()));
            match self.recv(cmd, wait)? {
                Some(p) if p[1] & 0x80 == 0 => {
                    let t = (p[2] & 0x7F) as usize;
                    if total.is_some_and(|old| old != t) {
                        chunks.clear(); // different reply shape; start over
                    }
                    total = Some(t);
                    let n = ((p[4] & 0x0F) as usize).min(CHUNK);
                    chunks.entry(p[3] as usize).or_insert_with(|| p[5..5 + n].to_vec());
                    if chunks.len() >= t {
                        return Ok((chunks, total));
                    }
                }
                _ => {} // timeout or CRC error reported by the device
            }
            if last_send.elapsed() >= resend_every && Instant::now() < deadline {
                last_send = Instant::now();
                self.send(&req)?;
            }
        }
        Ok((chunks, total))
    }

    pub fn set(&self, cmd: u8, data: &[u8], hi: u8, progress: &dyn Fn(usize, usize)) -> io::Result<()> {
        let n = data.len().div_ceil(CHUNK);
        for (idx, chunk) in data.chunks(CHUNK).enumerate() {
            let p = packet(cmd, n, idx, hi, chunk);
            let mut ok = false;
            // Re-sending a packet is idempotent (same index), so lost acks are harmless.
            let (tries, wait) = match self.link {
                Link::Receiver | Link::Wired => (6, Duration::from_secs(1)),
                Link::Bluetooth => (12, Duration::from_millis(1500)),
            };
            for _ in 0..tries {
                sleep(Duration::from_millis(15));
                self.send(&p)?;
                if let Some(ack) = self.recv(cmd, wait)? {
                    if ack[2] & 0x80 == 0 {
                        ok = true;
                        break;
                    }
                }
            }
            if !ok {
                return Err(err(format!("keyboard did not acknowledge command 0x{cmd:02X} packet {idx}/{n}")));
            }
            progress(idx + 1, n);
        }
        Ok(())
    }

    /// 6-byte product signature (vendor GetPSD).
    pub fn read_psd(&self) -> io::Result<[u8; 6]> {
        let v = match self.link {
            Link::Wired => self.wired_read(W_PSD, 1, 6)?,
            _ => self.request(CMD_PSD, 6, 6, Duration::from_millis(800), 2)?,
        };
        let mut a = [0u8; 6];
        a.copy_from_slice(&v);
        Ok(a)
    }

    /// Receiver: ask it whether the keyboard is linked (cmd 0x07).
    /// Bluetooth: the keyboard answers itself, so probe with GetPSD like the
    /// vendor DetectOnLine does for bBT devices.
    pub fn is_online(&self) -> bool {
        match self.link {
            Link::Wired => self.read_psd().is_ok(),
            Link::Receiver => matches!(self.get(CMD_ONLINE, 1, Duration::from_millis(600), 2), Ok(v) if v[0] != 0),
            Link::Bluetooth => self.read_psd().is_ok(),
        }
    }
    pub fn require_online(&self) -> io::Result<()> {
        if self.is_online() {
            Ok(())
        } else {
            Err(err("the keyboard is offline or asleep - press a key to wake it up"))
        }
    }
    /// Reads the 128-byte config. Over a lossy Bluetooth link a few packets may
    /// never arrive; those gaps are filled from the last complete config read
    /// from this keyboard (cached on disk). `self.filled` reports how many.
    pub fn read_config(&self) -> io::Result<LedConfig> {
        if self.link == Link::Wired {
            let v = self.wired_read(W_LED, 0, 128)?;
            let mut a = [0u8; 128];
            a.copy_from_slice(&v);
            let c = LedConfig(a);
            if !c.valid() {
                return Err(err("keyboard returned an invalid lighting config"));
            }
            self.filled.set(0);
            save_config_cache(&c);
            return Ok(c);
        }
        let (chunks, total) = self.collect(CMD_LED | READ, 0, Duration::from_secs(1), 20)?;
        let Some(t) = total else { return Err(err("no reply from keyboard for command 0x44")) };
        let complete = chunks.len() >= t;
        let cache = if complete { None } else { load_config_cache() };
        if !complete && cache.is_none() {
            return Err(err(format!(
                "incomplete reply from keyboard ({}/{t} packets) and no cached config to fill the gaps; try again or use the 2.4 GHz receiver once",
                chunks.len()
            )));
        }
        let mut a = cache.map(|c| c.0).unwrap_or([0u8; 128]);
        for (&idx, data) in &chunks {
            let off = idx * CHUNK;
            if off < 128 {
                let end = (off + data.len()).min(128);
                a[off..end].copy_from_slice(&data[..end - off]);
            }
        }
        self.filled.set(t.saturating_sub(chunks.len()));
        let c = LedConfig(a);
        if !c.valid() {
            return Err(err("keyboard returned an invalid lighting config"));
        }
        if complete {
            save_config_cache(&c);
        }
        Ok(c)
    }
    /// Every packet is acknowledged, so after success `c` is the keyboard's
    /// state; it becomes the gap-filling cache for lossy Bluetooth reads.
    pub fn write_config(&self, c: &LedConfig, pr: &dyn Fn(usize, usize)) -> io::Result<()> {
        match self.link {
            Link::Wired => self.wired_write(W_LED, 0, &c.0, pr)?,
            _ => self.set(CMD_LED, &c.0, 0, pr)?,
        }
        save_config_cache(c);
        Ok(())
    }
    pub fn write_palette(&self, pal: &[[[u8; 3]; 7]], pr: &dyn Fn(usize, usize)) -> io::Result<()> {
        let t = build_palette_table(pal);
        match self.link {
            Link::Wired => self.wired_write(W_PALETTE, 0, &t, pr),
            _ => self.set(CMD_PALETTE, &t, 0, pr),
        }
    }
    pub fn write_custom_colors(&self, c: &BTreeMap<usize, [u8; 3]>, pr: &dyn Fn(usize, usize)) -> io::Result<()> {
        let planes = build_custom_planes(c);
        match self.link {
            Link::Wired => self.wired_write(W_CUSTOM, 0, &planes, pr),
            _ => self.set(CMD_CUSTOM, &planes, 0, pr),
        }
    }
    pub fn write_matrix(&self, m: &[u8], layer: u8, pr: &dyn Fn(usize, usize)) -> io::Result<()> {
        match self.link {
            Link::Wired => self.wired_write(W_MATRIX, layer, m, pr),
            _ => self.set(CMD_MATRIX, m, layer, pr),
        }
    }
    /// Vendor "Reset using cmd": wired 0x11 with one byte, wireless a bare
    /// 0x06 request; no reply either way.
    pub fn factory_reset(&self) -> io::Result<()> {
        if self.link == Link::Wired {
            self.wired_write(W_RESET, 0, &[0], &|_, _| {})?;
            sleep(Duration::from_millis(500));
            return Ok(());
        }
        self.send(&packet(CMD_RESET, 1, 0, 0, &[]))?;
        sleep(Duration::from_millis(500));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wired_header() {
        // the lighting-config read, as sent by the vendor tool and verified on hardware
        let h = Device::wired_header(W_LED | W_READ, 0, 1, 0, 128);
        assert_eq!(h.len(), 520);
        assert_eq!(&h[..8], &[0x06, 0x84, 0x00, 0x00, 0x01, 0x00, 0x80, 0x00]);
    }

    #[test]
    fn packet_checksum() {
        let p = packet(0x07, 1, 0, 0, &[]);
        assert_eq!(p[..5], [0x13, 0x07, 0x01, 0x00, 0x00]);
        assert_eq!(p[19], 0x1b);
    }

    #[test]
    fn default_matrix() {
        let m = build_matrix(&BTreeMap::new(), &layout());
        assert_eq!(m.len(), 504);
        assert_eq!(&m[4..8], &[0, 0, 0x29, 0]); // Esc at matrix 1
        assert_eq!(&m[20..24], &[0, 0, 0xE0, 0]); // LCtrl at matrix 5
        assert_eq!(&m[59 * 4..60 * 4], &[0x0D, 0, 0, 0]); // Fn
    }

    #[test]
    fn palette_order() {
        let t = build_palette_table(&vec![DEFAULT_PALETTE; EFFECTS.len()]);
        assert_eq!(t.len(), 420);
        assert_eq!(&t[0x15..0x18], &[0x00, 0xFF, 0x00]); // red stored as G,R,B
    }
}
