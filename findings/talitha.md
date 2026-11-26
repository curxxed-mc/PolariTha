# AQIRYS Talitha

A tri-mode keyboard (USB cable, 2.4 GHz receiver, Bluetooth) with 67 keys on a
68-key layout. It has three USB/HID identities:

| Connection | ID | Name it reports |
|------------|----|-----------------|
| USB cable | `258a:0236` | "BY Tech USB Keyboard" |
| 2.4 GHz receiver | `3554:fa09` | "CX 2.4G Wireless Receiver" |
| Bluetooth (classic) | `3554:fa08` (bus 0005) | "Talitha BT3.0" |

`258a` is the vendor ID used by Sinowealth-based boards, and the vendor
software is a re-skin of a widely used OEM configurator (`Cfg.ini` names its
data folder `RKKB`). That matters for other AQIRYS keyboards: they are likely to
share this protocol with different `KB.ini` data.

## The vendor software

`DeviceDriver.exe` (an MFC application) plus data files:

- `Cfg.ini` (UTF-16): the model list (`DevName1=0236,AQIRYS Talitha`),
  `VID=0x258A`, and `DisableBT=1`.
- `Dev/0236/KB.ini`: everything model-specific, including the firmware family
  (`Fw=1`), the product signature (`Psd=3,0,0,0,3,5E`), the RGB byte order
  (`RGBIndex=1,0,2`), the effect capabilities (`LedOpt1..17`), a mask of hidden
  effects (`LedMask=0x2000`), and every key's rectangle, Windows virtual-key
  code and matrix index.
- `Text/*/text.xml` and `Dev/*/led.xml`: UI strings and effect names.

The program has three device classes, named in its RTTI and log strings:

- **`CDevG5KB`**: the keyboard logic, including `ApplySetting`, `FillCfg` and the
  key-code conversion, plus the wired transport.
- **`CDev3632`**: the wireless transport, used for the receiver and Bluetooth.
- **`CDevRKKB`**: an older wired family (`Fw=0`), which the Talitha doesn't use.

For a wireless connection, `CDevG5KB::ApplySetting` runs with a `CDev3632`
delegate attached, so both transports share all the settings code and differ
only in how bytes move.

## Wireless transport (receiver and Bluetooth)

Found from the 20-byte `WriteFile`/`ReadFile` pair in `FUN_0040e570` and
`FUN_0040e400`, and the callers that build and parse packets: `FUN_0040dc60`
(write), `FUN_0040df00` (read), `FUN_0040e690` (read with an argument byte).
**Vendor code**, and every packet type listed here was then **verified**.

- Report ID `0x13` on the `0xFF02` collection. Key, consumer and mouse reports
  arrive on the same hidraw node and have to be filtered out.
- Packet: `13 cmd npk idx (hi<<4 | len) data[14] sum`, where `sum` is the byte
  sum of bytes 0-18.
- A write is acknowledged per packet with the same `cmd`; bit 7 of byte 2 means
  CRC error. A read sends `cmd | 0x40`, and the device replies with `npk`
  packets.
- Packets whose `cmd` is `0x0A` are unsolicited notifications.

### Online check

`FUN_0040e3b0` sends command `0x07`. A data byte of 0 marks the device offline,
and Apply then refuses with "The device is offline and may be asleep". On the
first test the receiver answered `13 07 01 00 01 00 …`: the format matched, but
the keyboard was reported offline. **Verified**. The reason turned out to be
that the keyboard was connected over Bluetooth at the time.

### Settings

`ApplySetting` (`FUN_00412030`) reads the 128-byte config (`0x44`), patches it
with `FillCfg` (`FUN_00411d80` for `Fw=1`), and writes it back (`0x04`). Other
commands: palette (`0x09`), per-key colours (`0x02`), key matrix (`0x01`),
macros (`0x03`). **Vendor code**

- **Effect list:** the UI takes names from `tc_eft1..19` when `Fw` is non-zero.
  `LedMask` bit 13 hides effect 14 ("Retinue scanning"). `GaoshouIndex=17`
  (0-based) is "Self-define" (per-key colours), and the off index defaults to 18.
- **Mode byte:** all `LedOpt` effect codes are 0, so `FillCfg` stores the mode
  as `index + 1`, puts each effect's brightness and `speed<<4 | colour` pair at
  `0x3A + 2 × index`, and writes the self-define brightness at `0x5E` last.
  Reading the keyboard confirmed this: Colorful waterfall (index 15) read as
  byte `0x10` with `04 47` at `0x58`. **Verified**
- **Ranges:** brightness 0-4 and speed 1-4 come from defaults chosen when
  `Fw=1` (`BriNum`, `SpeedParam = 0x40401`).
- **Colours:** slots 0-6 index a seven-colour palette per effect, and 7 means
  rainbow. The default palette (red, green, blue, yellow, magenta, cyan, white)
  is set in `FUN_0045abb0`, which builds the default state.
- **Byte order:** `RGBIndex=1,0,2` places R at offset 1, G at 0 and B at 2, so
  palette and per-key colour data go over the wire as G, R, B.
- **Sleep:** the timer maps UI choices 1-5 to bytes `0x0A, 0x14, 0x28, 0x3C, 0`
  (10, 20, 40, 60 minutes, never).

### Key codes

`keyinfo_to_hardware_code` (`FUN_00410c00`, found through the `CDevG5KB`
vtable at `0x5ccac4`, slot `+0x60`) converts UI key entries to 4-byte codes.
**Vendor code**

| Code | Meaning |
|------|---------|
| `00 mods usage 00` | keyboard key with HID modifier bits |
| `02 00 hi lo` | consumer usage, big-endian |
| `31 mask 00 00` | mouse button |
| `0D 00 00 00` | Fn |

The knob press (virtual-key code `0x8F` in `KB.ini`) maps to consumer usage
`0xE2`, mute. The matrix holds 126 entries (`m_nMatrixLen`), placed by each
key's matrix index.

## Bluetooth

The keyboard pairs over classic Bluetooth as "Talitha BT3.0", `3554:fa08`. The
vendor tool only recognises BLE device paths containing `pid&fa07` as Bluetooth,
and `Cfg.ini` sets `DisableBT=1`, so the vendor never supported this connection
for the Talitha. **Vendor code**

What we found on hardware:

- **Same protocol.** Its report descriptor contains the same `0xFF02` / `0x13`
  collection, but not as the first collection, so device matching has to look
  at every collection. **Verified**
- **No online check.** In Bluetooth mode the vendor code skips command `0x07`
  (a question for the receiver) and uses GetPSD (`05`, argument 6) as the
  liveness check. The keyboard answered with `03 00 00 00 03 5E`, matching
  `KB.ini`. **Verified**
- **Lossy replies.** Multi-packet replies lose packets. A single config request
  produced only packet 0 of 10. `btmon` showed that only packet 0 reached the
  host over the air, so nothing on the Linux side was dropping them. BlueZ takes
  the link out of sniff mode when we write, and replies arrive in bursts about
  0.8 s apart, roughly as the link returns to sniff. **Verified**
- **Experiments:**
  - Changing the request's index byte doesn't select a packet; every reply
    starts again at 0.
  - Echoing acknowledgements back for received packets doesn't release the next
    one.
  - Re-requesting every 15-250 ms collects 7-9 of 10 packets in 12 s.
    **Verified**
- **What works.** Keep re-requesting and merge packets by index. Complete reads
  take about 20 s. If a few packets are still missing, they are filled from the
  last complete read cached on disk. **Verified**
- **Writes are reliable.** Each written packet is acknowledged, and in testing
  all 10 packets of a config write were acknowledged on the first try.
  **Verified**

The adapter used for testing was a Realtek combo card, which is a plausible
cause of the losses. A different Bluetooth adapter may behave better.

## USB cable

The vendor tool's `FindWired` (`FUN_00415b30`) walks the model list and,
for `Fw=1` models, creates a `CDevG5KB`. Its `FindHIDDevice` (`FUN_0040f7b0`)
opens the collection whose feature report is `0x208` bytes long and remembers
its report ID. `AccessData` (`FUN_0040fb60`) frames packets as
`rid cmd arg 00 npk idx len_lo len_hi` plus 512 data bytes. A write is one
SetFeature; a read is SetFeature, a 25 ms pause, then GetFeature. The command
numbers come from each wired call site: LED `04`, matrix `03`, macros `05`,
custom colours `06`, palette `0A`, PSD `82` (read) and reset `11`. **Vendor code**

Plugging in the cable produced `258a:0236` with the vendor collection on
interface 1, as feature report 6 (519 bytes plus the ID). Results:

- **Reads.** The PSD read returned `03 00 00 00 03 5E` and the config read
  returned all 128 bytes, with the PSD and config reads together taking 130 ms.
  **Verified**
- **Same layout as wireless.** The config matched the copy read over the
  receiver except byte `0x0E`, which is `00` over the cable and `01` over the
  receiver. **Verified**
- **Writes.** Changing brightness 3 → 4, reading it back and restoring 3 all
  worked. **Verified**

## Open questions

- **One reconnect.** The keyboard dropped off USB and reconnected once, about a
  second after a config write. Repeating the write did not reproduce it.
- **Untested over the cable.** Per-key colours, the palette, key matrix writes
  and factory reset haven't been exercised over the cable, but they use the
  same data as the wireless path.
- **LED indices.** `KB.ini` gives each key a single index, used here as both the
  matrix index and the LED index. Per-key colours rely on that assumption.
- **Config byte `0x0E`.** Possibly a connection-mode flag; it differs between
  cable and receiver (see above).
- **Unexplored.** Macros, Fn-layer editing, the keyboard's sleep notifications
  (`0x0A` packets) and music/real-time lighting (`0x88`).
- **BLE.** BLE (`fa07`) is inferred from the vendor code and has not been seen
  on hardware.
