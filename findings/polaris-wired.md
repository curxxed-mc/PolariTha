# AQIRYS Polaris Wired

A wired gaming mouse sold as "Polaris Wired". It shows up as **`04d9:a2e7`**
(Holtek vendor ID), named "E-Signal AQIRYS Polaris Wired Gaming Mouse". The
vendor files name the sensor as `Sensor=3370`, a PixArt PMW3370.

## The vendor software

The installer contains `MouseDriver.exe` (a DuiLib UI), a small HID helper
`hiddll.dll` (its internal name is `hiddriver.dll`), `Data.ini`, and
`Data/Default.pbin`, a 347,664-byte binary holding all five default profiles.

`MouseDriver.exe` does not import the HID API. It loads `hiddll.dll` at runtime
and resolves eleven exports with `GetProcAddress`: `Set_HIDVIDPID`,
`Open_HIDMonitor`, `Open_HIDFeatureDevice`, `SetHIDFeature`, `GetHIDFeature`,
`Open_HIDReportDevice`, `WriteHIDUSB`, `ReadHIDUSB` and their `Close_*`
counterparts. Decompiling the DLL showed:

- `SetHIDFeature` copies the caller's 8 bytes after a zero report ID and calls
  `HidD_SetFeature` with 9 bytes. **Vendor code**
- `WriteHIDUSB` writes 65 bytes: report ID 0 plus 64 data bytes. **Vendor code**
- `GetHIDFeature` and `ReadHIDUSB` exist, but **the application never calls
  them**. The vendor tool is write-only. It keeps the mouse's state on disk and
  re-sends everything on Apply, so PolariTha does the same. **Vendor code**

The interface is the third HID collection (interface 2): usage page `0xFF00`,
8-byte feature reports and 64-byte input/output reports, with no report IDs.
**Verified** (the report descriptor and successful writes)

## Commands and the checksum

All eight call sites of `SetHIDFeature` build an 8-byte buffer and fill the
last byte with an expression like `'v' - b2 - b3 - …`. Solving those for each
command gives a single rule: `b7 = 0xFF - sum(b0..b6)`. For example, `01 88`
gives `0xFF - 0x01 - 0x88 = 0x76 = 'v'`. **Vendor code**, and the mouse
accepted every write built this way. **Verified**

The Apply handler (`FUN_00415ac0`) sends, in order:

1. `01 88`: "start". It is also sent once at program start and again at the end.
2. `02 p`: select the active profile.
3. For each of the five profiles: `03 p rate 00` (polling rate), `04 p stage`
   (current DPI stage), then `0C p 80` followed by two 64-byte output reports
   (the 128-byte attribute block).
4. `0F n 80` macro blocks (`FUN_0041b740`), not implemented in PolariTha.
5. `0D p 80` plus 128 bytes for each profile's buttons (`FUN_0041ad40`).
6. `0C 00 80` plus the 128-byte global block (`FUN_0041a910`).
7. `01 88` and `02 p` again.

There are 20 ms pauses after feature reports and 30 ms after output reports;
PolariTha keeps them.

Polling-rate codes come from a switch in `FUN_0041a020`: UI index 0..3 maps to
8, 4, 2, 1, so the byte is the report interval in milliseconds (125 Hz to 1000 Hz).

## The attribute blocks

`Data.ini` holds two 128-byte templates, `[ATTRIBUTE_0]` (global) and
`[ATTRIBUTE_1]` (per profile), as eight lines of 16 hex bytes. The Apply code
copies a template and overwrites specific offsets with profile fields. Matching
those offsets to UI controls took two steps:

1. **Find which profile field each byte comes from.** For example
   `local_8a = *(iVar3 + 0x1b9 + base)` lands at block offset `0x46`.
2. **Find which control writes that field.** The page handlers (`FUN_0041d850`,
   `FUN_0041d190`, `FUN_0041cdd0`, `FUN_0040f5e0`, `FUN_0040f740`) compare the
   clicked control against stored pointers, and the init code
   (`FUN_0041c380`, `FUN_0040ee80`) shows which control name each pointer was
   looked up by (`"key_response_slider"`, `"max_dpi_combo"`, …).

That gave the per-profile layout in PROTOCOL.md: LOD enable at `0x41` bit 0,
stage count at `0x46`, light mode, brightness and speed at `0x47`-`0x49`, X/Y
sensitivity at `0x4A`/`0x4B`, DPI from `0x52`, LOD value at `0x66`, key response
at `0x67`, and DPI colours at `0x68`. **Vendor code**

Value ranges come from the UI XML (`res/performancepage.xml`,
`res/lightpage.xml`): X/Y sensitivity 1-100 %, key response 1-100 "ms",
brightness 0-255, speed 0-2, lift-off distance 0-1 (mapped through
`Distance_Value=1,2,…` to bytes 1 and 2). **Vendor code**

The global block is overwritten at `0x10`-`0x17` with a DPI-colour order read
from the profile file (`00 02 01 03 04 05 06 07`), and at `0x18`-`0x2F` with
up to eight RGB colours. Which colours are used depends on the light mode: Slide
uses one colour, and Standard, Wave and Finger movement use either eight copies
of the fixed colour or the eight cycle colours. **Vendor code**

## DPI encoding

`FUN_0041a190` contains a separate branch for each supported sensor, selected by
the `Sensor` value from `Data.ini` (0xd3d, 0xd2a, 0xd20, 0xcfd, 0xcff, 0xcee …).
For the 3370 (`0xd2a`), the UI slider runs 1-290, with labels from 50 to 19000:

- DPI is `50 × v` up to v = 200 (10,000 DPI), then `10000 + 100 × (v − 200)`.
- On the wire: `v ≤ 200` is sent as `v − 1`; above that as `v − 101` with the
  stage's bit set in a "double" mask (bytes `0x52`/`0x53`).

Both rules give the same steps, (byte + 1) × 50, doubled when the bit is set. A
unit test pins the encoding. **Vendor code**

## Buttons

The button menu (`res/buttonmenu.xml`) names each function. A long chain of
`CompareNoCase` calls in the menu handler maps those names to numeric function
IDs (`"click"` → 1, `"dpi_cycle"` → 6, `"juji"` → 113, …). A static table at
`0x426250` maps each ID to four wire bytes. **Vendor code**

Highlights:

- `01 00 F0..F4 00` are mouse buttons. Forward is `F4` and Backward is `F3`.
- `07 00 01/02/03 00` are DPI +, DPI − and DPI cycle; `08 …` is profile, `05 …`
  polling rate.
- `03 00 lo hi` is a consumer-page usage (media and application keys).
- `00 mods usage 00` is a keyboard key with HID modifier bits; the office
  shortcuts are stored this way (Ctrl+C = `00 01 06 00`).
- `0A F0 32 n` is double click (n = 2) or triple click (n = 3). Reading it as
  left button, 50 ms apart, n times is **inferred**.
- `00 00 00 00` disables the button.

Each profile has 16 entries. `Data.ini [BUTTON]` gives a default for each and a
flag for whether the user may change it: entries 1-9 are configurable (the UI
shows exactly nine buttons), and 15/16 are the wheel. Profile 2 lists the wheel
entries in the opposite order to the other profiles; PolariTha copies that as
it is.

## Hardware results

- **First write.** Applying a full configuration succeeded with no I/O errors,
  and the mouse stayed connected. **Verified**
- **Rapid-click timing.** Recording raw kernel events during fast alternating
  clicks showed a hard floor of about 15 ms for every press and every release,
  so presses were never closer than about 31 ms. USB polling is not the cause:
  the endpoint's `bInterval` is 1 (1000 Hz). Writing **key response = 1 ms**
  (`0x67`) and recording again did not move the floor. **Verified**

## Open questions

- What does key response (`0x67`) actually control? The vendor default is 20, but
  1 did not change the measured floor. The next test would be a large value such
  as 100 ms, to see whether the floor rises.
- Button 7's factory code `0B 09 00 00` is not in the function table. Type
  `0x0B` is the "sniper" function with argument 0, so 9 may be a sniper DPI
  stage, but that is unconfirmed.
- Most template bytes outside the mapped offsets (for example `0x42`-`0x45`:
  `0A 0A 19 19`) are written unchanged, and their meaning is unknown.
- Macros (`0F`), the recoil "gun" feature and the Office/Game button modes were
  traced only far enough to exclude them.
- The mouse sends notifications (DPI changed, profile changed) that the vendor
  UI handles in `FUN_00419ea0`. Where they arrive was not identified.
