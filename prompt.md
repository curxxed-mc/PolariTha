# Prompt: add an AQIRYS device to PolariTha

Give this file to an AI coding agent (for example: "Follow
https://github.com/curxxed-mc/PolariTha/blob/main/prompt.md for my AQIRYS
<model>"). Everything below is addressed to the agent.

---

You are helping the owner of an AQIRYS mouse, keyboard or headset get its
vendor configuration software working on Linux, as part of
[PolariTha](https://github.com/curxxed-mc/PolariTha), a Rust (egui) app that
talks to devices directly through `/dev/hidraw*`. Work on the owner's machine,
with the owner's device and the owner's copy of the vendor's Windows software.

## Before you start

1. Clone PolariTha and read `README.md`, `docs/PROTOCOL.md` and
   `docs/REVERSE_ENGINEERING.md`. `src/polaris.rs` (feature reports plus 64-byte
   bulk writes, write-only) and `src/talitha.rs` (request/acknowledge packets
   over a vendor report, receiver and Bluetooth) are worked examples of two
   different AQIRYS protocols. Expect a new device to differ from both.
2. Ask the owner for: the exact model, every connection mode it has (USB,
   2.4 GHz receiver, Bluetooth), and the path to the vendor installer. Do not
   download vendor software yourself unless the owner asks you to.
3. Agree on a scope with the owner: one feature on one connection mode is a
   good first goal (for example lighting over USB).

## Safety rules

- **Read before you write.** Use enumeration and documented read commands
  first. Never send a write, a reset or an unknown command to the device until
  you can explain every byte and the owner has said yes.
- Before the first write, record the current state and know how to restore it
  (a factory-reset command, the vendor tool on another machine, or a known-good
  full configuration).
- Do not attempt firmware updates, bootloader commands or flash writes.
- USB and Bluetooth captures can contain the owner's keystrokes. Keep raw
  captures local; quote only short excerpts of configuration traffic.
- Vendor installers, extracted executables, DLLs and decompiled output stay out
  of the repository. Only your own code and notes are committed.
- Installing udev rules or other system files needs the owner's approval.

## Workflow

1. **Find the configuration interface.** Run `cargo run -- list` and inspect
   `/sys/class/hidraw/hidrawN/device/uevent` and `report_descriptor`. Match the
   vendor collection by VID/PID, usage page and report ID, never by node number.
   Check every connection mode; the same collection may sit at a different
   position in the descriptor (the Talitha's does over Bluetooth).
2. **Unpack the vendor software.** Try `innoextract` first; otherwise do a
   silent install into a throwaway Wine prefix
   (`WINEPREFIX=$(mktemp -d) wine setup.exe /VERYSILENT /DIR='C:\vendor'`)
   without launching the program. Keep the result outside the repository
   (`ref/` and `software-ref/` are git-ignored).
3. **Read the data files.** `*.ini` and similar files often contain the USB
   IDs, default profiles, key layouts, effect lists and raw byte templates.
   Some are UTF-16.
4. **Decompile.** Run Ghidra headless and dump every function to text, then
   find the HID calls (`HidD_SetFeature`, `HidD_GetFeature`, `WriteFile`,
   `ReadFile`, or functions resolved with `GetProcAddress` from a helper DLL).
   Follow their callers to recover each packet: command byte, length, checksum,
   framing for multi-packet transfers, and the order of the Apply sequence.
   Map UI control or resource names to the fields they write.
5. **Confirm on hardware.** Implement the read path first and compare real
   replies with your hypothesis. Then, with the owner's approval, write one
   setting, verify the physical result, and restore the original. Use the `-v`
   flag to log raw traffic.
6. **Integrate.** Add `src/<device>.rs` with the protocol, a settings model and
   unit tests built from known bytes. Add a GUI page in `src/gui/` that uses the
   existing widgets and theme, a CLI command in `src/main.rs`, a udev rule in
   `packaging/70-polaritha.rules`, and a section in `docs/PROTOCOL.md`. Mark
   anything inferred but not tested on hardware as untested.
7. **Check.** `cargo build --release`, `cargo test --locked` and
   `cargo clippy` must pass.

## Hand back to the owner

Report what works and how you verified it on the device, what is untested,
and what you skipped. Then help the owner open a pull request from their fork
to the `main` branch of
[curxxed-mc/PolariTha](https://github.com/curxxed-mc/PolariTha) that names the
model, IDs and tested connection modes, explains the protocol briefly, and
includes a minimal annotated request/reply. Only open the pull request once the
owner has reviewed it.
