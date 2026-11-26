mod config;
mod gui;
mod hidraw;
mod keycodes;
mod polaris;
mod talitha;

use std::process::ExitCode;

const USAGE: &str = "\
polaritha - AQIRYS Polaris Wired / Talitha configurator

usage: polaritha [-v] [command]

  (no command)         start the graphical interface
  list                 list hidraw devices
  mouse-apply          write the saved mouse settings to the mouse
  mouse-profile N      switch the mouse to profile N (1-5)
  kb-status            show the keyboard's current lighting state
  kb-effect N [--brightness 0-4] [--speed 1-4] [--rainbow | --color RRGGBB]
                       set keyboard effect N (1-19, see kb-effects)
  kb-effects           list keyboard effects

  -v                   log raw HID traffic
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = if let Some(i) = args.iter().position(|a| a == "-v" || a == "--verbose") {
        args.remove(i);
        true
    } else {
        false
    };
    if args.is_empty() || args[0] == "gui" {
        return match gui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match cli(&args, verbose) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn cli(args: &[String], verbose: bool) -> Result<(), String> {
    let e = |e: std::io::Error| e.to_string();
    let noprog = |_: usize, _: usize| {};
    match args[0].as_str() {
        "-h" | "--help" | "help" => print!("{USAGE}"),
        "list" => {
            for i in hidraw::enumerate() {
                let has = |p| i.collections.contains(&p);
                let tag = if (i.vid, i.pid) == (polaris::VID, polaris::PID) && has(polaris::USAGE_PAGE) {
                    "  <- Polaris config interface"
                } else if (i.vid, i.pid) == (talitha::DONGLE_VID, talitha::DONGLE_PID) && has(talitha::USAGE_PAGE) {
                    "  <- Talitha receiver config interface"
                } else if (i.vid, i.pid) == (talitha::WIRED_VID, talitha::WIRED_PID) && has(talitha::WIRED_USAGE_PAGE) && i.report_ids.contains(&6) {
                    "  <- Talitha USB cable config interface"
                } else if i.vid == talitha::DONGLE_VID && talitha::BT_PIDS.contains(&i.pid) && has(talitha::USAGE_PAGE) {
                    "  <- Talitha Bluetooth config interface"
                } else {
                    ""
                };
                println!("{:14} {:04x}:{:04x} page=0x{:04X} ids={:?} {}{}", i.node, i.vid, i.pid, i.usage_page, i.report_ids, i.name, tag);
            }
        }
        "mouse-apply" => {
            let s = polaris::Settings::load();
            let mut d = polaris::Device::open().map_err(e)?;
            d.verbose = verbose;
            d.apply(&s, &noprog).map_err(e)?;
            println!("Settings written to the mouse.");
        }
        "mouse-profile" => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).filter(|n| (1..=5).contains(n)).ok_or("profile must be 1-5")?;
            let mut s = polaris::Settings::load();
            s.active_profile = n - 1;
            s.save();
            let mut d = polaris::Device::open().map_err(e)?;
            d.verbose = verbose;
            d.switch_profile(n - 1).map_err(e)?;
        }
        "kb-effects" => {
            for (i, f) in talitha::EFFECTS.iter().enumerate() {
                if i != talitha::HIDDEN_EFFECT {
                    println!("{:2}  {}", i + 1, f.name);
                }
            }
        }
        "kb-status" => {
            let mut d = talitha::Device::open().map_err(e)?;
            d.verbose = verbose;
            println!("Link:     {}", d.link.name());
            if !d.is_online() {
                println!("Keyboard: offline / asleep");
                return Ok(());
            }
            let c = d.read_config().map_err(e)?;
            let fx = c.effect();
            let (b, sp, col) = c.params(fx);
            println!("Keyboard: online");
            if d.filled.get() > 0 {
                println!("Note:     {} of 10 config packets were lost on the link and filled from the cached copy", d.filled.get());
            }
            if let Ok(psd) = d.read_psd() {
                let tag = if psd == talitha::TALITHA_PSD { "Talitha" } else { "unexpected" };
                println!("PSD:      {psd:02x?} ({tag})");
            }
            println!("Effect:   {} (brightness {b}, speed {sp}, colour {})", talitha::EFFECTS[fx].name,
                if col == talitha::RAINBOW { "rainbow".to_string() } else { format!("slot {}", col + 1) });
            let sl = talitha::SLEEP_OPTIONS.iter().find(|o| o.0 == c.sleep()).map(|o| o.1.to_string()).unwrap_or(format!("0x{:02x}", c.sleep()));
            println!("Sleep:    {sl}");
            if verbose {
                println!("{:02x?}", c.0);
            }
        }
        "kb-effect" => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).filter(|n| (1..=19).contains(n)).ok_or("effect must be 1-19")?;
            let fx = n - 1;
            let mut d = talitha::Device::open().map_err(e)?;
            d.verbose = verbose;
            d.require_online().map_err(e)?;
            let mut c = d.read_config().map_err(e)?;
            let (mut b, mut sp, mut col) = c.params(fx);
            if let Some(v) = opt(args, "--brightness") {
                b = v.parse().map_err(|_| "bad brightness")?;
            }
            if let Some(v) = opt(args, "--speed") {
                sp = v.parse().map_err(|_| "bad speed")?;
            }
            if args.iter().any(|a| a == "--rainbow") {
                col = talitha::RAINBOW;
            } else if let Some(hex) = opt(args, "--color") {
                let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).map_err(|_| "bad colour")?;
                let mut st = talitha::State::load();
                st.palettes[fx][0] = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                st.save();
                d.write_palette(&st.palettes, &noprog).map_err(e)?;
                col = 0;
            }
            c.set_effect(fx, b, sp, col);
            d.write_config(&c, &noprog).map_err(e)?;
            println!("Done.");
        }
        other => return Err(format!("unknown command '{other}'\n\n{USAGE}")),
    }
    Ok(())
}
