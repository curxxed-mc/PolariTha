# Reverse-engineering findings

[docs/PROTOCOL.md](../docs/PROTOCOL.md) is the compact reference: bytes, IDs and
offsets. This folder explains how those were found: where each fact came from,
how it was confirmed, the experiments behind the less obvious behaviour, the
dead ends, and what is still unknown. Read it before extending a device, or
when a detail in PROTOCOL.md looks surprising.

- [Polaris Wired mouse](polaris-wired.md)
- [Talitha keyboard](talitha.md)

## Evidence levels

Each finding is tagged with how sure we are:

- **Verified**: observed on real hardware with PolariTha (a reply was read, or a
  write changed the device and read back as expected).
- **Vendor code**: taken from the decompiled Windows software or its data files,
  but not exercised on hardware yet.
- **Inferred**: a reasonable reading of the evidence that neither of the above
  confirms.

## Source material

Both analyses started from the vendor's Windows installers. They are not part of
this repository; if you have the same files, these hashes tell you whether you
are looking at the same versions.

| File | SHA-256 |
|------|---------|
| `AQIRYS Polaris Wired Software Setup..rar` | `ba8c8f8bf5ae7006f7e652016a22356a99b55134eaa9787000b4ddc194a004b9` |
| `MouseDriver.exe` (Polaris) | `aec3f9f746e629d2096292db31c9fe6f4b6986b6c008935d745f38f66376af10` |
| `hiddll.dll` (Polaris) | `a0753c74dbd087bbc33012a3989bf8f944f7d722b4dcbb955909f83a714a6900` |
| `AQIRYS Talitha Software.rar` | `a788b372493fdb92e4072a09ec475fb27aa3362a725d437d33beabc4b7344d3d` |
| `DeviceDriver.exe` (Talitha) | `0054a5bfad0e9ecd34f95b78a1f80fea9de1bee96890e4dbb3eb07e1ef44db22` |

Function names such as `FUN_00415ac0` are Ghidra's default labels for these
exact binaries (32-bit, image base `0x400000`), so they can be looked up again.

## Method, in short

1. **Unpack.** Both installers are Inno Setup (5.5.7 for the Polaris, 5.3.3 for
   the Talitha). `innoextract` wasn't available, so each was silently installed
   into a throwaway Wine prefix (`/VERYSILENT /DIR=...`) and the files copied
   out. The programs themselves were never run.
2. **Read the data files.** The Polaris ships `Data.ini` and a binary default
   profile; the Talitha ships `Cfg.ini` (UTF-16) and a per-model `Dev/0236/KB.ini`.
   They held the USB IDs, default settings, key layouts and even raw byte
   templates.
3. **Decompile.** Ghidra 12 in headless mode dumped every function to text,
   which made it possible to grep for HID API calls, strings and constants.
4. **Trace the HID calls.** Starting from `HidD_SetFeature`, `HidD_GetFeature`,
   `WriteFile` and `ReadFile` (or the helper-DLL exports resolved with
   `GetProcAddress`), we followed the callers to the code that builds each
   buffer, then up to the UI handlers that fill in the values.
5. **Confirm on hardware.** Reads first, then single, reversible writes, logged
   with `polaritha -v`. Bluetooth behaviour was checked with `btmon`, and raw
   input timing with the kernel's evdev nodes.
