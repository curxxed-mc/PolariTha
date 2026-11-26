# Adding an AQIRYS device on Linux

Start with one feature you can see, such as changing a static LED colour. A useful first contribution can support one feature on one connection type. The [Polaris and Talitha protocol notes](PROTOCOL.md) show how differently two AQIRYS products encode similar settings.

## Identify the configuration interface

Connect the device and record its exact model, hardware revision if printed, and connection mode. For USB, run `lsusb` and note the vendor and product IDs. From the project directory, run `cargo run -- list` to enumerate hidraw nodes. Compare the list before and after connecting the device. A receiver can expose several interfaces and can stay visible while its peripheral sleeps.

For a candidate such as `hidraw3`, inspect its identity and report descriptor locally:

```sh
node=hidraw3
cat "/sys/class/hidraw/$node/device/uevent"
od -Ax -tx1 -v "/sys/class/hidraw/$node/device/report_descriptor"
```

Inspect the HID collections, usage pages, report IDs, report types and lengths. Match those properties in code because node numbers can change after reconnection. Repeat discovery for wired, receiver and Bluetooth modes you intend to support.

## Read the vendor software

This is how the Polaris and Talitha protocols were recovered, and it needs no
Windows machine. AQIRYS utilities are usually Inno Setup installers wrapping a
vendor program, a HID helper DLL and plain-text data files.

1. **Extract the installer.** `innoextract` unpacks Inno Setup installers
   directly. If it rejects the version, install into a throwaway Wine prefix
   without running the program: `WINEPREFIX=$(mktemp -d) wine setup.exe
   /VERYSILENT /DIR='C:\vendor'`, then copy the files out of
   `drive_c/vendor`.
2. **Read the data files first.** Files such as `Data.ini`, `Cfg.ini` or
   `KB.ini` often hold the USB IDs, default profiles, key layouts and even the
   raw byte templates the program sends. Check their text encoding; some are
   UTF-16.
3. **Find the HID calls.** Note the imports of the executable and its DLLs
   (`HidD_SetFeature`, `HidD_GetFeature`, `WriteFile`, `ReadFile`), or names
   resolved with `GetProcAddress`. Decompile with Ghidra (headless mode can
   dump every function to text), then follow the callers of those functions.
   The buffer they build is your packet: look for the command byte, length
   fields and checksum, and for the order in which an Apply button sends them.
4. **Map fields to the UI.** Search the decompilation for control or resource
   names (`"dpi_slider"`, `"tc_led%d"`) to see which settings write which bytes,
   and how slider values are converted before they reach the wire.

Treat decompiled code as a hypothesis: confirm each field with a capture or on
the device before you rely on it. Keep the vendor files and decompiled output
local; only your own notes and code belong in the repository.

## Observe a known setting change



For a USB device, one option on a Linux host is to pass it through to a Windows VM running the vendor utility. Use `lsusb` to find the bus and address, load the Linux capture module with `sudo modprobe usbmon`, and capture the corresponding `usbmonN` bus in Wireshark. Filter the display to the device address. USB passthrough gives the guest control of that peripheral, so use a second input device to operate the host.

1. Record an idle baseline, apply one setting, and record the original and new values. Repeat with a third value, then restore the original. Keep the selected profile and other settings constant.
2. Compare host-to-device control transfers (including HID Set_Report) and interrupt OUT data. Pair them with device replies. Record the report type, ID, payload length, byte order and any repeated prefix or trailing checksum. Distinguish USB submission and completion records so you count each transfer once.
3. Follow the whole Apply sequence. Some utilities write an entire profile for a single slider change. Compare repeat runs to distinguish the setting from counters, checksums and acknowledgements. Mark inferred fields as tentative until another value confirms them.

On Bluetooth, `btmon` can record host traffic during an operation on Linux. Wireless and receiver traces may have different framing. Keep original captures local. A bus capture can contain keystrokes from other devices even when a Wireshark display filter hides them. Share a short, annotated excerpt containing only the configuration reports.

## Implement and verify one operation

Use [hidraw.rs](../src/hidraw.rs) for Linux transport and the [Polaris](../src/polaris.rs) and [Talitha](../src/talitha.rs) modules as device-specific examples. Linux hidraw writes and feature-report ioctls take a leading report ID byte, including zero for an unnumbered report. Account for that byte when comparing your buffer with a USB capture.

Start with enumeration and a documented status read if available. Before writing, record the original setting and identify how to restore it. For a full configuration write, preserve unrelated fields from a known configuration. Leave operations with unknown side effects out of the first implementation.

Run PolariTha with `-v` to log every report it sends and receives. Check the generated report against the observed bytes, then apply the change to your exact device. Verify the physical result, restore the original value, and reconnect to check persistence if the feature claims it. Record which cases you actually tested, including any lost replies or unsupported connection modes.

## Send the working feature upstream

Fork [PolariTha](https://github.com/curxxed-mc/PolariTha), create a branch in your fork, and commit the implementation and protocol notes. Include a small test using known report bytes when adding an encoder or decoder. Run `cargo test --locked`, push the branch, and open a pull request from your branch to the `main` branch of [curxxed-mc/PolariTha](https://github.com/curxxed-mc/PolariTha).

In the PR, name the model, IDs and tested connection mode. Describe the feature, give steps another owner can use to reproduce your result, and include a minimal annotated request/reply or expected packet. Explain how you verified the result on hardware and list any untested variants. Include original code and observations. Keep vendor installers, extracted executables and full captures out of the PR.
