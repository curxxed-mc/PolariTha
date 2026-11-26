//! JSON settings in $XDG_CONFIG_HOME/polaritha/.

use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("polaritha")
}

pub fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    std::fs::read_to_string(dir().join(name)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save<T: Serialize>(name: &str, value: &T) {
    let d = dir();
    if std::fs::create_dir_all(&d).is_err() {
        return;
    }
    let tmp = d.join(format!("{name}.tmp"));
    if let Ok(s) = serde_json::to_string_pretty(value) {
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, d.join(name));
        }
    }
}
