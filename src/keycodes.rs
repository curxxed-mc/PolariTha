//! HID usage tables shared by both devices.

pub const MODIFIERS: [(u8, &str); 8] = [
    (0x01, "Ctrl"), (0x02, "Shift"), (0x04, "Alt"), (0x08, "Super"),
    (0x10, "RCtrl"), (0x20, "RShift"), (0x40, "RAlt"), (0x80, "RSuper"),
];

/// HID keyboard-page usage -> name.
pub fn key_name(u: u8) -> String {
    match u {
        0x04..=0x1D => ((b'A' + (u - 0x04)) as char).to_string(),
        0x1E..=0x26 => ((b'1' + (u - 0x1E)) as char).to_string(),
        0x27 => "0".into(),
        0x3A..=0x45 => format!("F{}", u - 0x39),
        0x68..=0x73 => format!("F{}", u - 0x68 + 13),
        0x59..=0x61 => format!("KP {}", u - 0x58),
        0x62 => "KP 0".into(),
        _ => match u {
            0x28 => "Enter", 0x29 => "Esc", 0x2A => "Backspace", 0x2B => "Tab", 0x2C => "Space",
            0x2D => "-", 0x2E => "=", 0x2F => "[", 0x30 => "]", 0x31 => "\\", 0x33 => ";",
            0x34 => "'", 0x35 => "`", 0x36 => ",", 0x37 => ".", 0x38 => "/", 0x39 => "Caps Lock",
            0x46 => "Print Screen", 0x47 => "Scroll Lock", 0x48 => "Pause", 0x49 => "Insert",
            0x4A => "Home", 0x4B => "Page Up", 0x4C => "Delete", 0x4D => "End", 0x4E => "Page Down",
            0x4F => "Right", 0x50 => "Left", 0x51 => "Down", 0x52 => "Up", 0x53 => "Num Lock",
            0x54 => "KP /", 0x55 => "KP *", 0x56 => "KP -", 0x57 => "KP +", 0x58 => "KP Enter",
            0x63 => "KP .", 0x64 => "ISO \\", 0x65 => "Menu",
            0xE0 => "Left Ctrl", 0xE1 => "Left Shift", 0xE2 => "Left Alt", 0xE3 => "Left Super",
            0xE4 => "Right Ctrl", 0xE5 => "Right Shift", 0xE6 => "Right Alt", 0xE7 => "Right Super",
            _ => return format!("Key 0x{u:02X}"),
        }
        .into(),
    }
}

/// All selectable keyboard usages, in a sensible order.
pub fn all_keys() -> Vec<u8> {
    let mut v: Vec<u8> = (0x04..=0x65).filter(|&u| u != 0x32).collect();
    v.extend(0x68..=0x73);
    v.extend(0xE0..=0xE7);
    v
}

pub fn describe_combo(mods: u8, usage: u8) -> String {
    let mut parts: Vec<String> = MODIFIERS.iter().filter(|(b, _)| mods & b != 0).map(|(_, n)| n.to_string()).collect();
    if usage != 0 {
        parts.push(key_name(usage));
    }
    if parts.is_empty() { "None".into() } else { parts.join("+") }
}

/// Consumer-page usages offered for remapping.
pub const CONSUMER: [(u16, &str); 21] = [
    (0x00CD, "Play / pause"), (0x00B5, "Next track"), (0x00B6, "Previous track"), (0x00B7, "Stop"),
    (0x00E2, "Mute"), (0x00E9, "Volume up"), (0x00EA, "Volume down"),
    (0x006F, "Screen brightness up"), (0x0070, "Screen brightness down"),
    (0x0183, "Media player"), (0x018A, "E-mail"), (0x0192, "Calculator"), (0x0194, "My computer"),
    (0x0221, "Browser search"), (0x0223, "Browser home"), (0x0224, "Browser back"),
    (0x0225, "Browser forward"), (0x0226, "Browser stop"), (0x0227, "Browser refresh"),
    (0x022A, "Browser favorites"), (0x0000, ""),
];

pub fn consumer_name(u: u16) -> String {
    CONSUMER.iter().find(|(c, n)| *c == u && !n.is_empty()).map(|(_, n)| n.to_string()).unwrap_or(format!("Consumer 0x{u:04X}"))
}

/// Windows virtual-key code (vendor KB.ini) -> HID usage.
pub fn vk_to_hid(vk: u8) -> u8 {
    match vk {
        0x41..=0x5A => vk - 0x41 + 0x04,
        0x31..=0x39 => vk - 0x31 + 0x1E,
        0x30 => 0x27,
        0x70..=0x7B => vk - 0x70 + 0x3A,
        0x0D => 0x28, 0x1B => 0x29, 0x08 => 0x2A, 0x09 => 0x2B, 0x20 => 0x2C, 0xBD => 0x2D,
        0xBB => 0x2E, 0xDB => 0x2F, 0xDD => 0x30, 0xDC => 0x31, 0xBA => 0x33, 0xDE => 0x34,
        0xC0 => 0x35, 0xBC => 0x36, 0xBE => 0x37, 0xBF => 0x38, 0x14 => 0x39, 0x2C => 0x46,
        0x91 => 0x47, 0x13 => 0x48, 0x2D => 0x49, 0x24 => 0x4A, 0x21 => 0x4B, 0x2E => 0x4C,
        0x23 => 0x4D, 0x22 => 0x4E, 0x27 => 0x4F, 0x25 => 0x50, 0x28 => 0x51, 0x26 => 0x52,
        0x90 => 0x53, 0x5D => 0x65,
        0xA2 => 0xE0, 0xA0 => 0xE1, 0xA4 => 0xE2, 0x5B => 0xE3,
        0xA3 => 0xE4, 0xA1 => 0xE5, 0xA5 => 0xE6, 0x5C => 0xE7,
        _ => 0,
    }
}
