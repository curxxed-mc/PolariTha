# Protocol notes

Recovered from the vendor Windows binaries with Ghidra. The function names below
refer to that decompilation.

## AQIRYS Polaris Wired — 04d9:a2e7 (Holtek MCU, PixArt PMW3370)

Interface: the HID collection on usage page `0xFF00` (interface 2). It has 8-byte
feature reports and 64-byte input/output reports, with no report IDs.

### Commands (feature report, 8 bytes)

`b7 = (0xFF - sum(b0..b6)) & 0xFF`

| bytes            | meaning                                                    |
|------------------|------------------------------------------------------------|
| `01 88`          | start / unlock (sent before and after an apply)            |
| `02 p`           | select active profile `p` = 1..5                           |
| `03 p r 00`      | polling rate, `r` = 8/4/2/1 for 125/250/500/1000 Hz        |
| `04 p s`         | current DPI stage `s` = 1..8                               |
| `0C p 80`        | followed by a 128-byte attribute block (p = 0 for global)  |
| `0D p 80`        | followed by a 128-byte button block                        |
| `0F n 80`        | followed by 128 bytes of macro data (not implemented)      |

The 128-byte blocks are sent as two 64-byte output reports, with 20–30 ms between
writes. The apply order (FUN_00415ac0) is: `01 88`, `02 cur`, for each profile
`03`/`04`/`0C`, then `0D` for each profile, then the global `0C 00`, then `01 88`
and `02 cur` again.

### Per-profile attribute block (template `[ATTRIBUTE_1]` in Data.ini)

| off       | field                                                            |
|-----------|------------------------------------------------------------------|
| 0x41 bit0 | lift-off distance enabled                                        |
| 0x46      | number of DPI stages                                             |
| 0x47      | light mode 0..11                                                 |
| 0x48      | brightness 0..255                                                |
| 0x49      | light speed 0..2                                                 |
| 0x4A/4B   | X/Y sensitivity %                                                |
| 0x52/53   | per-stage "double" bit mask (X/Y)                                |
| 0x54..5B  | DPI X per stage, 0x5C..63 DPI Y                                  |
| 0x66      | LOD value (1 low, 2 high)                                        |
| 0x67      | key response (ms)                                                |
| 0x68..7F  | 8 × RGB stage colours                                            |

DPI for the 3370: the slider value `v` runs 1..290, where DPI = 50·v up to 10000 and
10000 + 100·(v−200) above that. On the wire, `v ≤ 200` is sent as `v−1`; otherwise
it's sent as `v−101` with the stage's double bit set.

### Global attribute block (`[ATTRIBUTE_0]`)

| off      | field                                                    |
|----------|----------------------------------------------------------|
| 0x10..17 | DPI colour order (`00 02 01 03 04 05 06 07`)             |
| 0x18..2F | 8 × RGB light colours (Slide uses only the first)        |
| 0x30     | `FF`; 0x32..3B are zero                                  |

### Button block

16 entries × 4 bytes. Entries 1–9 are user buttons and 15/16 are the wheel.
Codes: `01 00 F0..F4 00` mouse buttons, `04 00 01/02 00` scroll, `07 00 1/2/3 00`
DPI +/−/cycle, `08 …` profile, `05 …` polling rate, `03 00 lo hi` consumer usage,
`00 mods usage 00` keyboard, `0A F0 32 n` multi-click, `0C` light mode switch,
`00 00 00 00` disabled.

## AQIRYS Talitha — receiver 3554:fa09 ("CDev3632")

Interface: the collection on usage page `0xFF02`, report ID `0x13`, with 19-byte
input and output reports. Key, consumer and mouse reports also arrive on the same node.

Packet: `13 cmd npk idx (hi<<4|len) data[14] sum`, where `sum = sum(b0..b18) & 0xFF`.
Each written packet is acknowledged by a packet with the same `cmd`; a set bit 7 in
byte 2 means a CRC error. A read sends `cmd|0x40` with `npk = 1`, and the reply
comes back in `npk` packets with up to 14 bytes each. Packets with `cmd == 0x0A`
are unsolicited notifications.

| cmd       | payload                                                              |
|-----------|----------------------------------------------------------------------|
| 0x01      | key matrix, `hi` = layer, 126 × 4 bytes                              |
| 0x02      | per-key colour planes, `hi` = group, 3 × 126 bytes (G,R,B plane order) |
| 0x03      | macro data, `hi` = page, 512-byte pages (not implemented)            |
| 0x04/0x44 | 128-byte config, ends with `5A A5`                                   |
| 0x06      | factory reset (no reply)                                             |
| 0x07      | 1 byte; non-zero = keyboard linked and awake                         |
| 0x09      | palette: 21 zero bytes + 19 effects × 7 colours × 3 bytes (G,R,B)    |

Config (FillCfg, Fw=1): `[0x09]` self-define flag, `[0x0A]` effect index+1
(0 = off), `[0x0B]` 0x20 + colour group, `[0x18]` sleep (0x0A/0x14/0x28/0x3C/0),
`[0x3A + 2e]` brightness for effect `e`, `[0x3B + 2e]` speed<<4 | colour slot
(7 = rainbow), `[0x5E]` self-define brightness.

Key codes (keyinfo_to_hardware_code): `00 mods usage 00` keyboard,
`02 00 hi lo` consumer, `31 mask 00 00` mouse button, `0D 00 00 00` Fn.

### Bluetooth

Over BT the keyboard exposes the same `0xFF02` / report `0x13` collection. It is
not the first collection in its descriptor, so it has to be matched by
collection rather than by the first usage page. The vendor tool flags a path
containing `pid&fa07` (BLE) as `bBT`; classic BT shows up as `3554:fa08`. The
differences in BT mode:

* cmd 0x07 (asks the receiver whether the keyboard is linked) is skipped, and
  liveness is checked with GetPSD instead: `13 05 01 00 06 …` returns 10 bytes,
  the first 6 being `03 00 00 00 03 5E`, matching `Psd=` in KB.ini
* real-time (music) lighting is disabled

The request's index byte is ignored: every read restarts the reply at packet 0.
Multi-packet replies arrive in bursts tied to the BT sniff interval (~0.8 s),
and packets inside a burst are frequently lost before reaching the host
(verified with btmon). The fix is to re-request and merge packets by index.
Writes are acknowledged per packet, and re-sending a packet is idempotent.

### USB cable (258a:0236, "CDevG5KB")

Plugged in by cable, the keyboard enumerates as a Sinowealth-based
`258A:0236` "BY Tech USB Keyboard". Configuration goes through vendor feature
report **6** on interface 1: a `0xFF00` collection whose feature report is 519
bytes plus the ID (`0x208` = 520 bytes in total).

Header: `06 cmd arg 00 npk idx len_lo len_hi`, followed by up to 512 data bytes.
A write is one SetFeature per packet. A read sends the header with `cmd | 0x80`,
waits ~25 ms, then reads feature report 6 back; byte 1 echoes the command and
the data starts at byte 8. There are no acknowledgements or checksums.

| cmd   | payload                                                  | wireless equivalent |
|-------|----------------------------------------------------------|---------------------|
| 0x02  | PSD, read only (`0x82`, arg 1, 6 bytes)                  | 0x05                |
| 0x03  | key matrix, arg = layer, 126 × 4 bytes                   | 0x01                |
| 0x04  | 128-byte config (read `0x84`)                            | 0x04 / 0x44         |
| 0x05  | macro data (not implemented)                             | 0x03                |
| 0x06  | per-key colour planes, arg = group                       | 0x02                |
| 0x0A  | per-effect palette table                                 | 0x09                |
| 0x11  | factory reset (1 byte)                                   | 0x06                |

The config, palette, colour planes and key matrix have the same layout as over
the receiver. Byte `0x0E` of the config reads `00` over the cable and `01` over
the receiver; PolariTha doesn't use it.
