//! Minimal Linux hidraw access: sysfs enumeration + read/write/feature ioctls.

use std::fs;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct HidrawInfo {
    pub node: String,
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    /// First top-level usage page (for display).
    pub usage_page: u32,
    /// Every usage page that opens a top-level collection.
    pub collections: Vec<u32>,
    pub report_ids: Vec<u8>,
}

/// Usage pages of all top-level collections, and all report IDs.
fn parse_descriptor(d: &[u8]) -> (Vec<u32>, Vec<u8>) {
    let mut page = 0u32;
    let mut depth = 0i32;
    let mut cols = Vec::new();
    let mut ids = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let prefix = d[i];
        if prefix == 0xFE {
            if i + 1 >= d.len() {
                break;
            }
            i += 3 + d[i + 1] as usize;
            continue;
        }
        let size = [0usize, 1, 2, 4][(prefix & 3) as usize];
        let mut v: u32 = 0;
        for k in 0..size {
            if let Some(b) = d.get(i + 1 + k) {
                v |= (*b as u32) << (8 * k);
            }
        }
        match prefix & 0xFC {
            0x04 => page = v,
            0x84 => ids.push(v as u8),
            0xA0 => {
                if depth == 0 {
                    cols.push(page);
                }
                depth += 1;
            }
            0xC0 => depth -= 1,
            _ => {}
        }
        i += 1 + size;
    }
    (cols, ids)
}

pub fn enumerate() -> Vec<HidrawInfo> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return out };
    let mut entries: Vec<_> = dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    entries.sort_by_key(|n| n.trim_start_matches("hidraw").parse::<u32>().unwrap_or(0));
    for name in entries {
        let dev = Path::new("/sys/class/hidraw").join(&name).join("device");
        let (Ok(uevent), Ok(desc)) = (fs::read_to_string(dev.join("uevent")), fs::read(dev.join("report_descriptor"))) else {
            continue;
        };
        let mut vid = 0;
        let mut pid = 0;
        let mut hname = String::new();
        for line in uevent.lines() {
            if let Some(v) = line.strip_prefix("HID_ID=") {
                let p: Vec<&str> = v.split(':').collect();
                if p.len() == 3 {
                    vid = u32::from_str_radix(p[1], 16).unwrap_or(0) as u16;
                    pid = u32::from_str_radix(p[2], 16).unwrap_or(0) as u16;
                }
            } else if let Some(v) = line.strip_prefix("HID_NAME=") {
                hname = v.to_string();
            }
        }
        let (collections, report_ids) = parse_descriptor(&desc);
        let usage_page = collections.first().copied().unwrap_or(0);
        out.push(HidrawInfo { node: format!("/dev/{name}"), vid, pid, name: hname, usage_page, collections, report_ids });
    }
    out
}

/// First hidraw node of vid:pid that has a top-level collection on `usage_page`.
pub fn find(vid: u16, pid: u16, usage_page: u32) -> Option<HidrawInfo> {
    enumerate().into_iter().find(|i| i.vid == vid && i.pid == pid && i.collections.contains(&usage_page))
}

// _IOC(dir, type, nr, size)
const fn ioc(dir: u64, ty: u64, nr: u64, size: u64) -> u64 {
    (dir << 30) | (size << 16) | (ty << 8) | nr
}
fn hidiocsfeature(len: usize) -> u64 {
    ioc(3, b'H' as u64, 0x06, len as u64)
}

pub struct Hidraw {
    fd: OwnedFd,
}

impl Hidraw {
    pub fn open(node: &str) -> io::Result<Self> {
        let f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(node)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    io::Error::new(
                        e.kind(),
                        format!("no permission to open {node}; install packaging/70-polaritha.rules and re-plug the device"),
                    )
                } else {
                    e
                }
            })?;
        Ok(Self { fd: f.into() })
    }

    fn poll(&self, events: i16, timeout: Duration) -> io::Result<bool> {
        let mut p = libc::pollfd { fd: self.fd.as_raw_fd(), events, revents: 0 };
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let r = unsafe { libc::poll(&mut p, 1, ms) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        if p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "device disconnected"));
        }
        Ok(r > 0)
    }

    /// Write an output report; `data[0]` is the report id (0 if unused).
    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let n = unsafe { libc::write(self.fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
            if n >= 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::WouldBlock || Instant::now() > deadline {
                return Err(e);
            }
            self.poll(libc::POLLOUT, Duration::from_millis(50))?;
        }
    }

    /// Read one input report, `None` on timeout.
    pub fn read(&self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        if !self.poll(libc::POLLIN, timeout)? {
            return Ok(None);
        }
        let mut buf = [0u8; 256];
        let n = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(e) };
        }
        Ok(Some(buf[..n as usize].to_vec()))
    }

    pub fn drain(&self) {
        while let Ok(Some(_)) = self.read(Duration::ZERO) {}
    }

    /// Send a feature report; `data[0]` is the report id.
    pub fn set_feature(&self, data: &[u8]) -> io::Result<()> {
        let mut buf = data.to_vec();
        let r = unsafe { libc::ioctl(self.fd.as_raw_fd(), hidiocsfeature(buf.len()) as _, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
