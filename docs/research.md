# Flydigi Vader 4 Pro / Vader 5 Pro — Reverse Engineering & gilrs Integration Research

## Sources Investigated

| Source | Type | URL |
|--------|------|-----|
| DuncanTPerkins/vader5-pro-linux-fix | SDL3 patch (Linux) | https://github.com/DuncanTPerkins/vader5-pro-linux-fix |
| DrProton824/vader5pro-hid-tools | Python HID reader (Windows) | https://github.com/DrProton824/vader5pro-hid-tools |
| dracinn/ControlLab | macOS bridge app | https://github.com/dracinn/ControlLab |
| BANANASJIM/padctl | Linux multi-device driver | (GitHub, referenced by SDL) |
| SDL3 SDL_hidapi_flydigi.c | Official SDL HIDAPI driver | SDL source tree |
| Reddit r/Controller | Reverse engineering discussion | reddit.com/r/Controller/comments/1v3c4b2 |

---

## 1. VID/PID Combinations

| Controller | VID | PID | Transport | Protocol | Status |
|------------|-----|-----|-----------|----------|--------|
| Vader 4 Pro (USB direct / XInput) | `0x37D7` | `0x3001` | USB / 2.4G (Flydigi receiver) | V1 | CONFIRMED |
| Vader 4 Pro (DInput / Cypress 2.4G) | `0x04B4` | `0x2412` | 2.4G USB dongle (Cypress chip) | V1 | CONFIRMED |
| Vader 5 Pro (2.4G receiver) | `0x37D7` | `0x2401` | 2.4G USB receiver | V2 | CONFIRMED |
| Vader 5 Pro charging dock | `0x37D7` | `0x6001` | USB (dock) | N/A | CONFIRMED |

Note: `0x04B4` is Cypress Semiconductor (the receiver chip vendor). Multiple Flydigi controllers share this VID/PID and are distinguished by device ID in an info response.

SDL Device IDs:
- Vader 4 Pro: 85, 91, 105
- Vader 5 Pro: 130
- Vader 3 Pro: 80, 81
- Vader 2 Pro: 22

---

## 2. Protocol Generations

Two distinct HID protocol generations exist:

### V1 Protocol (Vader 2/3/3 Pro/4 Pro, Apex 2/3/4)

- **Report framing:** Numbered HID reports (Report ID `0x04`)
- **Initialization:** NOT required — reports flow immediately
- **Vendor interface:** Same as gamepad (Interface 0 for USB, Interface 2 for DInput/2.4G)
- **Report size:** 32 bytes
- **Stick encoding:** u8 (center 0x7F, range 0x00-0xFF)
- **Trigger encoding:** u8 (offsets 23-24)
- **Extra button byte:** Offset 7

### V2 Protocol (Vader 5 Pro, Apex 5)

- **Report framing:** Unnumbered 32-byte reports with magic `5A A5`
- **Initialization:** REQUIRED — 5-command handshake to enable `0xEF` input stream
- **Vendor interface:** Separate Interface 1, usage page `0xFFA0`, usage `0x0001`
- **Report size:** 32 bytes
- **Stick encoding:** int16 LE (native signed)
- **Trigger encoding:** u8 (offsets 15-16)
- **Extra button byte:** Offset 13

---

## 3. USB Interface Layout

### Vader 4 Pro (`0x37D7:0x3001`)

```
Interface 0: HID gamepad + vendor protocol (all buttons including extras)
```

No separate vendor interface needed — extra buttons are in the same report.

### Vader 4 Pro DInput (`0x04B4:0x2412`)

```
Interface 2: HID — vendor protocol with extended buttons
```

### Vader 5 Pro (`0x37D7:0x2401`)

```
Interface 0: XInput gamepad (standard buttons only)
Interface 1: Vendor HID (0xFFA0) — all buttons including extras + IMU
Interface 2: Mouse HID (unused)
Interface 3: Vendor HID 0xFFEE (unknown purpose)
```

---

## 4. Report Byte Layouts

### Vader 4 Pro — V1 Report (Report ID 0x04, 32 bytes)

```
Offset  Size  Content
------  ----  -------
0       1     Report ID (0x04)
7       1     Extended buttons (M1-M4, C/Z)
8       1     System buttons (Home, Select/Quick)
9       1     Face buttons (Y, B, A) + DPad (hat enum in bits 4-7)
10      1     Shoulder/stick buttons (RS/LS/RT/LT/RB/LB/Start/X)
11-16   6     Accelerometer X/Y/Z (3x int16 LE)
17      1     Left stick X (u8, center 0x7F)
19      1     Left stick Y (u8)
21      1     Right stick X (u8)
22      1     Right stick Y (u8)
23      1     Left trigger (u8)
24      1     Right trigger (u8)
26-27   2     Gyro X (int16 LE)
29-30   2     Gyro Z (int16 LE)
```

#### Byte 7 — Vader 4 Pro Extra Buttons

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | C      | CONFIRMED (SDL has_cz=true) |
| 1   | 0x02 | Z      | CONFIRMED (SDL has_cz=true) |
| 2   | 0x04 | M1     | CONFIRMED (padctl + SDL) |
| 3   | 0x08 | M2     | CONFIRMED |
| 4   | 0x10 | M3     | CONFIRMED |
| 5   | 0x20 | M4     | CONFIRMED |
| 6   | 0x40 | —      | UNKNOWN |
| 7   | 0x80 | —      | UNKNOWN |

#### Byte 8 — System Buttons

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | Home   | CONFIRMED |
| 1   | 0x02 | Select | CONFIRMED |

#### Byte 9 — Face + DPad

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | Y      | CONFIRMED |
| 2   | 0x04 | B      | CONFIRMED |
| 3   | 0x08 | A      | CONFIRMED |
| 4-7 | 0xF0 | DPad   | CONFIRMED (hat enum) |

#### Byte 10 — Shoulders, Sticks, Start, X

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | RS     | CONFIRMED |
| 1   | 0x02 | LS     | CONFIRMED |
| 2   | 0x04 | RT     | CONFIRMED |
| 3   | 0x08 | LT     | CONFIRMED |
| 4   | 0x10 | RB     | CONFIRMED |
| 5   | 0x20 | LB     | CONFIRMED |
| 6   | 0x40 | Start  | CONFIRMED |
| 7   | 0x80 | X      | CONFIRMED |

### Vader 5 Pro — V2 Report (Unnumbered, 32 bytes)

```
Offset  Size  Content
------  ----  -------
0-2     3     Magic: 5A A5 EF
3-4     2     Left stick X (int16 LE)
5-6     2     Left stick Y (int16 LE, negate)
7-8     2     Right stick X (int16 LE)
9-10    2     Right stick Y (int16 LE, negate)
11      1     Buttons 1 (DPad + A/B/Select/X)
12      1     Buttons 2 (Y/Start/LB/RB/LS/RS)
13      1     Extended buttons (C/Z/M1-M4/LM/RM)
14      1     Extended buttons 2 (Function/Home)
15      1     Left trigger (u8)
16      1     Right trigger (u8)
17-22   6     Gyroscope X/Z/Y (3x int16 LE)
23-28   6     Accelerometer X/Z/Y (3x int16 LE)
29-31   3     Reserved
```

#### Byte 11 — Standard Buttons 1

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | UP     | CONFIRMED |
| 1   | 0x02 | RIGHT  | CONFIRMED |
| 2   | 0x04 | DOWN   | CONFIRMED |
| 3   | 0x08 | LEFT   | CONFIRMED |
| 4   | 0x10 | A      | CONFIRMED |
| 5   | 0x20 | B      | CONFIRMED |
| 6   | 0x40 | SELECT | CONFIRMED |
| 7   | 0x80 | X      | CONFIRMED |

#### Byte 12 — Standard Buttons 2

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | Y      | CONFIRMED |
| 1   | 0x02 | START  | CONFIRMED |
| 2   | 0x04 | LB     | CONFIRMED |
| 3   | 0x08 | RB     | CONFIRMED |
| 4   | 0x10 | LT     | CONFIRMED (digital) |
| 5   | 0x20 | RT     | CONFIRMED (digital) |
| 6   | 0x40 | LS     | CONFIRMED |
| 7   | 0x80 | RS     | CONFIRMED |

#### Byte 13 — Extended Buttons 1

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | C      | CONFIRMED (all sources) |
| 1   | 0x02 | Z      | CONFIRMED (all sources) |
| 2   | 0x04 | M1     | CONFIRMED (all sources) |
| 3   | 0x08 | M2     | CONFIRMED (all sources) |
| 4   | 0x10 | M3     | CONFIRMED (all sources) |
| 5   | 0x20 | M4     | CONFIRMED (all sources) |
| 6   | 0x40 | LM     | CONFIRMED (all sources) |
| 7   | 0x80 | RM     | CONFIRMED (all sources) |

#### Byte 14 — Extended Buttons 2

| Bit | Mask | Button | Status |
|-----|------|--------|--------|
| 0   | 0x01 | O/Fn/Circle | CONFIRMED |
| 1   | 0x02 | Arrow  | CONFIRMED |
| 3   | 0x08 | Home   | CONFIRMED |

---

## 5. Initialization / Handshake

### Vader 4 Pro (V1): NO INITIALIZATION REQUIRED

Reports flow immediately upon opening the HID interface. SDL V1 sends an info query (`0x05 0xEC ...`) to identify the device model, but this is optional and not required for input.

### Vader 5 Pro (V2): 5-COMMAND HANDSHAKE REQUIRED

Each command is zero-padded to 32 bytes, prefixed with `5A A5`:

```
# Command                                    Purpose
1 5A A5 01 02 03                             Firmware version query
2 5A A5 A1 02 A3                             Device info / MAC query
3 5A A5 02 02 04                             Status/config query
4 5A A5 04 02 06                             Config data query
5 5A A5 11 07 FF 01 FF FF FF 15             Enable extended input stream
```

Stop command: `5A A5 11 07 FF 00 FF FF FF 14`

Checksum format: wrapping 8-bit additive sum of bytes from command byte through last param byte.

ControlLab inserts 60ms delay between each command. vader5pro-hid-tools delays 5 seconds after first traffic before sending init.

Note from vader5pro-hid-tools: "our Windows hidapi backend already decodes buttons correctly without sending anything" — the init may not be universally required, but is needed to guarantee the `0xEF` report stream.

SDL V2 also sends an **acquire** command (heartbeat every 30s):
```
03 5A A5 1C 17 01 <app_name_padded_to_26_bytes>
```
The controller must have "Allow third-party apps" enabled in SpaceStation for this to work.

---

## 6. Transport Differences

| Feature | USB Direct | 2.4G Receiver | Bluetooth |
|---------|-----------|---------------|-----------|
| Vader 4 Pro extra buttons | YES | YES (via Cypress dongle) | UNKNOWN |
| Vader 5 Pro extra buttons | N/A (uses receiver) | YES | **NO** (Xbox BT profile) |
| Vader 4 Pro init needed | No | No | UNKNOWN |
| Vader 5 Pro init needed | N/A | Yes | N/A |
| Vader 5 Pro IMU | N/A | Yes | No |
| Vader 5 Pro BT profile | N/A | N/A | Standard Xbox/GIP |

**CRITICAL: Bluetooth does NOT expose the vendor HID interface. Extra buttons are USB-only (direct or receiver).**

---

## 7. Key Differences: Vader 4 Pro vs Vader 5 Pro

| Property | Vader 4 Pro | Vader 5 Pro |
|----------|-------------|-------------|
| Protocol generation | V1 | V2 |
| VID:PID | `37D7:3001` or `04B4:2412` | `37D7:2401` |
| Init required | **No** | **Yes** |
| Report header | `0x04` (report ID) | `5A A5 EF` (magic) |
| Stick encoding | u8 (center 0x7F) | int16 LE |
| Extra buttons byte offset | 7 | 13 |
| Has C, Z | **Yes** (SDL has_cz) | Yes |
| Has M1-M4 | **Yes** | Yes |
| Has LM, RM | **No** | Yes |
| Has Circle/O | **No** | Yes |
| M5, M6 | **No** | **No evidence** |
| Vendor interface | Same as gamepad (IF0/IF2) | Separate (IF1, 0xFFA0) |
| Report ID used | Yes (0x04) | No (unnumbered) |

---

## 8. M5/M6 Investigation

**No evidence of M5 or M6 buttons exists in any examined source:**
- Not in SDL source code
- Not in padctl configs
- Not in ControlLab
- Not in vader5pro-hid-tools
- Not in any reverse engineering documentation
- Not in Reddit discussions

The `has_lmrm` flag in SDL (for Vader 5 Pro / Apex 5) maps to LM/RM buttons (shoulder bumper tops), NOT M5/M6.

**Status: M5/M6 likely do not exist on current hardware revisions.**

---

## 9. Existing gilrs Architecture

### Current state
- **Linux backend:** Pure evdev (`/dev/input/eventXX`) via raw ioctls. No hidraw/hidapi.
- **macOS backend:** IOKit HID (`IOHIDManager`) for standard gamepad matching.
- **Windows backend:** WGI (`Windows.Gaming.Input`) for standard gamepad API.
- **No Flydigi-specific support** anywhere in the codebase.
- **No vendor HID support** on any platform.

### Button enum
gilrs already has `Button::C` and `Button::Z` (from traditional 6-button fighting controller layout). These map to evdev `BTN_C` (0x132) and `BTN_Z` (0x135).

gilrs does NOT have `Button::M1` through `Button::M6`, `Button::LM`, or `Button::RM`.

### How new controllers are normally added
1. SDL GameControllerDB mapping string (maps evdev codes to logical buttons)
2. This only works for buttons the kernel already exposes via evdev

### The problem
The Linux kernel's generic HID driver does NOT understand Flydigi's vendor protocol. The extra buttons (C, Z, M1-M4) are in a vendor-specific report that the kernel drops or ignores. They never appear as evdev events.

For Vader 5 Pro, the vendor interface (Interface 1, usage page 0xFFA0) is a completely separate endpoint that the standard gamepad driver never touches.

---

## 10. Feasibility Assessment

### Can the Vader 5 Pro extra-button approach work for Vader 4 Pro?

**YES.** Both controllers expose extra buttons via vendor-specific HID reports that the standard gamepad stack ignores. The approach is identical in concept:

1. Detect Flydigi device by VID/PID
2. Open the vendor HID interface (hidraw on Linux, IOKit on macOS, HID API on Windows)
3. For V2 (Vader 5 Pro): Send initialization sequence
4. For V1 (Vader 4 Pro): No init needed
5. Decode the vendor report to extract extra button state
6. Merge extra button events with the normal gamepad stream
7. Expose as new `Button` variants

### Key differences that affect implementation

| Aspect | Impact |
|--------|--------|
| Different report formats | Need separate V1 and V2 decoders |
| Different PIDs | Device detection must handle multiple VID/PID pairs |
| V1 no init needed | Simpler — just open and read |
| V1 vendor on same interface | May need different hidraw enumeration strategy |
| V4 Pro has no LM/RM | Capability detection per model |

### Architecture required in gilrs

```
gilrs-core
├── platform/
│   ├── linux/
│   │   ├── gamepad.rs        (existing evdev)
│   │   └── flydigi_hid.rs    (NEW: hidraw reader)
│   ├── macos/
│   │   ├── gamepad.rs        (existing IOKit)
│   │   └── flydigi_hid.rs    (NEW: IOHIDDevice vendor reader)
│   └── windows_wgi/
│       ├── gamepad.rs        (existing WGI)
│       └── flydigi_hid.rs    (NEW: HID API vendor reader)
└── flydigi/                   (NEW: cross-platform protocol module)
    ├── mod.rs
    ├── detect.rs              (VID/PID matching, model detection)
    ├── protocol_v1.rs         (Vader 4 Pro decoder)
    ├── protocol_v2.rs         (Vader 5 Pro decoder)
    └── types.rs               (FlydigiInput, FlydigiButtons, etc.)
```

### Button API design

gilrs already has `Button::C` and `Button::Z`. For M1-M4 (and LM/RM), new variants would need to be added to the `Button` enum:

```rust
pub enum Button {
    // ... existing variants ...
    C,       // already exists
    Z,       // already exists
    // NEW:
    M1,
    M2,
    M3,
    M4,
    // Vader 5 Pro only:
    LeftMacro,   // LM
    RightMacro,  // RM
}
```

This is a semver-breaking change since `Button` is `#[non_exhaustive]` — wait, actually checking the source: `Button` does NOT have `#[non_exhaustive]`. It uses `#[repr(u16)]` with explicit discriminant values. Adding new variants would be a semver concern.

Alternative: Use `Button::Unknown` with a distinguishable `Code` for each extra button, leveraging the existing `(Button, Code)` tuple in events. This avoids API breakage but makes pattern matching harder.

---

## 11. Supported Button Matrix

| Controller | USB | Receiver | Bluetooth | C | Z | M1 | M2 | M3 | M4 | LM | RM | M5 | M6 |
|------------|-----|----------|-----------|---|---|----|----|----|----|----|----|----|----|
| Vader 4 Pro | CONFIRMED | CONFIRMED | UNKNOWN | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | N/A | N/A | N/A | N/A |
| Vader 5 Pro | N/A (receiver) | CONFIRMED | NO | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | CONFIRMED | NO EVIDENCE | NO EVIDENCE |

---

## 12. Risks and Open Questions

1. **Vader 4 Pro Bluetooth:** No source confirms whether BT exposes vendor HID. Status: UNKNOWN.
2. **Vader 4 Pro C/Z conflict:** padctl configs don't map C/Z for Vader 4 Pro, but SDL sets `has_cz=true`. May depend on firmware revision.
3. **Vader 4 Pro vendor interface location:** V1 protocol puts extra buttons in the same interface as standard gamepad. The kernel may already process this interface, making hidraw access tricky (shared vs exclusive).
4. **SDL acquire heartbeat:** V2 may require periodic heartbeat commands. If the controller has "Allow third-party apps" disabled in SpaceStation, the vendor stream may not work.
5. **Firmware revisions:** Different device IDs (85, 91, 105 for Vader 4 Pro) may have slightly different report layouts.
6. **gilrs `Button` enum extensibility:** Adding new variants is a semver concern. The `#[repr(u16)]` layout and exhaustive match patterns in downstream code would break.
7. **hidraw permissions on Linux:** Reading `/dev/hidraw*` typically requires root or udev rules granting access.
8. **Concurrent access:** The vendor interface can be read alongside the normal gamepad driver without conflict (CONFIRMED by all sources), but SpaceStation on Windows may compete for the device.

---

## 13. Recommended Implementation Order

1. **Phase 1:** Cross-platform protocol decoder module (V1 + V2) with comprehensive unit tests
2. **Phase 2:** Linux hidraw integration for Vader 5 Pro (V2 — best documented, separate interface)
3. **Phase 3:** Linux hidraw integration for Vader 4 Pro (V1 — simpler protocol, shared interface)
4. **Phase 4:** macOS IOKit integration (both V1/V2)
5. **Phase 5:** Windows HID API integration (both V1/V2)
6. **Phase 6:** Button API design (new variants or Code-based approach)
7. **Phase 7:** Documentation and CI

---

## 14. Reference Implementations Comparison

| Feature | SDL3 (linux-fix) | vader5pro-hid-tools | ControlLab | padctl |
|---------|-------------------|---------------------|------------|--------|
| Language | C | Python | Swift/ObjC | Unknown |
| Platform | Linux | Windows | macOS | Linux |
| V1 support | Yes | No | No | Yes |
| V2 support | Yes | Yes | Yes | Yes |
| Init sequence | Yes | Yes | Yes | Yes (V2) |
| Vader 4 Pro | Yes (via V1) | No | No | Yes |
| Vader 5 Pro | Yes | Yes | Yes | Yes |
| Extra buttons | Full decode | Full decode | Full decode | Full decode |
| Rumble | Yes | Experimental | No | Unknown |
| IMU | Yes | No | Yes | Yes |
| Virtual gamepad | No (native SDL) | No (keyboard remap) | Yes (IOKit) | Yes (uinput) |

> **Status note.** This is the initial investigation, kept for the record; it has not
> been re-verified line by line. Where it disagrees with [flydigi.md](flydigi.md), the
> latter wins - it was checked against SDL3, `dantmnf/Vader4ProReader` and
> `BANANASJIM/flydigi-vader5`. Known differences:
>
> * The claim that the Vader 4 Pro also appears as `0x37D7:0x3001` is **UNVERIFIED**.
>   That combination is not present in SDL3's `usb_ids.h`, in the Linux kernel's
>   `hid-ids.h` or in any other examined implementation, so it is not used by the
>   implementation.
> * The byte 9 / byte 10 breakdown in section 4 does not match SDL3, which reads
>   D-Pad + A/B/Select/X from byte 9 and Y/Start/LB/RB/LS/RS from byte 10.
> * Section 2/3 describe "V1"/"V2" protocol generations; the authoritative version is
>   in [flydigi.md](flydigi.md).
