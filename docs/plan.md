# Implementation Plan: Flydigi Vader 4 Pro (V1 Protocol) Extra Button Support

## Goal

Expose Vader 4 Pro extra buttons (C, Z, M1, M2, M3, M4) as first-class gilrs events on Linux, macOS, and Windows without requiring external remapping software.

## V1 Protocol Summary

| Property | Value |
|----------|-------|
| VID:PID | `0x37D7:0x3001` (USB) or `0x04B4:0x2412` (Cypress 2.4G) |
| Init required | **No** — reports flow immediately |
| Report framing | Report ID `0x04`, 32 bytes |
| Extra buttons byte | Offset 7 |
| Stick encoding | u8 (center 0x7F) |
| Vendor interface | Same as gamepad (IF0 or IF2) |
| Buttons available | C, Z, M1, M2, M3, M4 |

### Byte 7 bit layout

| Bit | Mask | Button |
|-----|------|--------|
| 0   | 0x01 | C      |
| 1   | 0x02 | Z      |
| 2   | 0x04 | M1     |
| 3   | 0x08 | M2     |
| 4   | 0x10 | M3     |
| 5   | 0x20 | M4     |

---

## Phase 0 — Button API Changes

### Problem

`Button` is `#[repr(u16)]` and exhaustive. Adding M1-M4 requires new variants.

### Changes

**`gilrs/src/constants.rs`**

```rust
pub const BTN_M1: u16 = 20;
pub const BTN_M2: u16 = 21;
pub const BTN_M3: u16 = 22;
pub const BTN_M4: u16 = 23;
```

**`gilrs/src/ev/mod.rs`** — add `#[non_exhaustive]` and new variants:

```rust
#[non_exhaustive]
#[repr(u16)]
pub enum Button {
    // ... existing ...
    C = BTN_C,       // already exists
    Z = BTN_Z,       // already exists
    M1 = BTN_M1,     // NEW
    M2 = BTN_M2,     // NEW
    M3 = BTN_M3,     // NEW
    M4 = BTN_M4,     // NEW
    // ...
}
```

Add helper:

```rust
pub fn is_macro(self) -> bool {
    matches!(self, M1 | M2 | M3 | M4)
}
```

**`gilrs-core` native_ev_codes** — add `BTN_M1`..`BTN_M4` per platform.

**`gilrs/src/mapping/mod.rs`** — extend `from_data()` and `default()` for new variants.

### Files changed

- `gilrs/src/constants.rs`
- `gilrs/src/ev/mod.rs`
- `gilrs-core/src/lib.rs`
- `gilrs-core/src/platform/linux/gamepad.rs`
- `gilrs-core/src/platform/macos/gamepad.rs`
- `gilrs-core/src/platform/windows_wgi/gamepad.rs`
- `gilrs-core/src/platform/windows_xinput/gamepad.rs`
- `gilrs-core/src/platform/default/gamepad.rs`
- `gilrs-core/src/platform/wasm/gamepad.rs`
- `gilrs/src/mapping/mod.rs`

### Note

This phase is shared with the V2 plan. If implementing both, do this once. The V2 plan adds `BTN_LM`/`BTN_RM` and `Button::LeftMacro`/`Button::RightMacro` on top.

---

## Phase 1 — V1 Protocol Decoder

Pure Rust module, no platform dependencies. Operates on `&[u8]`.

### Location

`gilrs-core/src/flydigi/`

### Files

**`gilrs-core/src/flydigi/mod.rs`**

```rust
pub mod detect;
pub mod protocol_v1;
pub mod types;
```

**`gilrs-core/src/flydigi/detect.rs`**

```rust
pub const FLYDIGI_VID: u16 = 0x37D7;
pub const CYPRESS_VID: u16 = 0x04B4;
pub const VADER4_PRO_PID: u16 = 0x3001;
pub const VADER4_PRO_DINPUT_PID: u16 = 0x2412;

pub fn is_vader4_pro(vendor_id: u16, product_id: u16) -> bool {
    matches!(
        (vendor_id, product_id),
        (FLYDIGI_VID, VADER4_PRO_PID) | (CYPRESS_VID, VADER4_PRO_DINPUT_PID)
    )
}
```

**`gilrs-core/src/flydigi/types.rs`**

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct V1Buttons {
    pub c: bool,
    pub z: bool,
    pub m1: bool,
    pub m2: bool,
    pub m3: bool,
    pub m4: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1ButtonId { C, Z, M1, M2, M3, M4 }

pub enum ButtonEvent {
    Pressed(V1ButtonId),
    Released(V1ButtonId),
}

pub struct V1State {
    prev: V1Buttons,
}

impl V1State {
    pub fn new() -> Self { Self { prev: V1Buttons::default() } }

    pub fn update(&mut self, current: V1Buttons) -> impl Iterator<Item = ButtonEvent> {
        // Compare each field, emit Pressed/Released
        // ...
        self.prev = current;
        events.into_iter()
    }
}
```

**`gilrs-core/src/flydigi/protocol_v1.rs`**

```rust
const REPORT_ID: u8 = 0x04;
const EXTRA_BUTTONS_OFFSET: usize = 7;

pub fn decode_extra_buttons(report: &[u8]) -> Option<V1Buttons> {
    if report.len() < 8 { return None; }
    if report[0] != REPORT_ID { return None; }
    let b = report[EXTRA_BUTTONS_OFFSET];
    Some(V1Buttons {
        c:  b & 0x01 != 0,
        z:  b & 0x02 != 0,
        m1: b & 0x04 != 0,
        m2: b & 0x08 != 0,
        m3: b & 0x10 != 0,
        m4: b & 0x20 != 0,
    })
}
```

### Unit tests

- Every individual bit (0x01, 0x02, 0x04, 0x08, 0x10, 0x20)
- Combinations (0x03, 0x0C, 0x30, 0x3F)
- All zeros → no buttons
- All ones (0xFF) → all six buttons
- Short report (len < 8) → None
- Wrong report ID → None
- Edge detection: press, release, hold, simultaneous changes

### Files added

- `gilrs-core/src/flydigi/mod.rs`
- `gilrs-core/src/flydigi/detect.rs`
- `gilrs-core/src/flydigi/types.rs`
- `gilrs-core/src/flydigi/protocol_v1.rs`

---

## Phase 2 — Linux: evdev path

### Strategy

V1 puts extra buttons in the **same HID report and same USB interface** as standard gamepad buttons. The kernel's `hid-generic` driver parses the HID report descriptor and creates evdev events. No init needed. This is the simplest path.

### Step 2a — Verify kernel behavior (requires hardware)

```bash
# Find the Vader 4 Pro evdev device
sudo evtest
# Select the Vader 4 Pro device
# Press M1, M2, M3, M4, C, Z — record evdev codes
```

Expected outcomes:
- **Best case:** Kernel maps extra buttons to evdev codes in `BTN_MISC` (0x100-0x10F) or `BTN_TRIGGER_HAPPY` (0x2C0+) range. gilrs already picks these up via `find_buttons()`.
- **Worst case:** Kernel ignores the extra buttons entirely (vendor usage page in descriptor). Need hidraw fallback.

### Step 2b — If kernel exposes buttons: SDL mapping string

Add to `gamecontrollerdb.txt` or gilrs custom mappings:

```
<uuid>,Flydigi Vader 4 Pro,a:b0,b:b1,...,c:bN,z:bM,...
```

Where N, M are the button indices for the evdev codes discovered in step 2a.

Also add `native_ev_codes` mapping for M1-M4 using the actual evdev codes:

```rust
pub const BTN_M1: EvCode = EvCode { kind: EV_KEY, code: <actual_code> };
pub const BTN_M2: EvCode = EvCode { kind: EV_KEY, code: <actual_code> };
pub const BTN_M3: EvCode = EvCode { kind: EV_KEY, code: <actual_code> };
pub const BTN_M4: EvCode = EvCode { kind: EV_KEY, code: <actual_code> };
```

Include these in `Mapping::default()` when VID/PID matches Vader 4 Pro.

**This may be zero code changes beyond mapping strings and native_ev_codes constants.**

### Step 2c — If kernel does NOT expose buttons: hidraw fallback

Open `/dev/hidraw*` for the Vader 4 Pro. Since V1 needs no init, this is read-only:

1. Detect Flydigi VID/PID in `Gamepad::open()`
2. Find corresponding hidraw device via sysfs
3. Open hidraw fd (non-blocking)
4. Add to epoll alongside evdev fd
5. On hidraw readable: decode V1 report → inject extra button events

```rust
// In platform/linux/gamepad.rs Gamepad struct:
flydigi_hidraw_fd: Option<RawFd>,
flydigi_state: Option<V1State>,
```

When epoll fires for the hidraw fd, read 32 bytes, call `protocol_v1::decode_extra_buttons()`, compare with `V1State`, emit events.

**hidraw permission:** Requires udev rule:
```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="37d7", ATTRS{idProduct}=="3001", MODE="0666"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="04b4", ATTRS{idProduct}=="2412", MODE="0666"
```

### Files changed

- `gilrs-core/src/platform/linux/gamepad.rs` (native_ev_codes, detection, optional hidraw)
- SDL `gamecontrollerdb.txt` (mapping string)

---

## Phase 3 — macOS: IOKit path

### Strategy

IOKit provides full HID access. V1 extra buttons are on the same device as standard gamepad, so the existing IOHIDManager already matches the device. The extra buttons should appear as IOKit elements.

### Step 3a — Check what IOKit exposes (requires hardware)

Run IOKit HID exploration on a Vader 4 Pro to see what elements are enumerated. If the extra buttons appear as `kHIDPage_Button` elements, they're already visible to gilrs.

### Step 3b — If elements are on vendor page: expand element handling

The current `element_is_button()` in `io_kit.rs` only accepts `kHIDPage_Button` and `kHIDPage_Consumer`. If the extra buttons use a vendor usage page, add:

```rust
fn element_is_button(element: &IOHIDElement) -> bool {
    let page = element.get_usage_page();
    matches!(page, kHIDPage_Button | kHIDPage_Consumer)
        || is_flydigi_vendor_button(element)
}

fn is_flydigi_vendor_button(element: &IOHIDElement) -> bool {
    let page = element.get_usage_page();
    let device = element.get_device();
    let vid = device.get_vendor_id();
    let pid = device.get_product_id();
    page >= kHIDPage_VendorDefinedStart && detect::is_vader4_pro(vid, pid)
}
```

### Step 3c — Map vendor elements to native_ev_codes

Map the IOKit element usage → gilrs EvCode for M1-M4:

```rust
// In platform/macos/gamepad.rs native_ev_codes
pub const BTN_M1: EvCode = EvCode(/* platform-specific value */);
// ...
```

The exact mapping depends on what IOKit enumerates. If elements are on a vendor page, their usage values determine the mapping.

### Step 3d — If elements are NOT individually enumerated: raw report fallback

Register `IOHIDDeviceRegisterInputReportCallback` on the device. Receive raw 32-byte reports. Decode with `protocol_v1::decode_extra_buttons()`. Inject events.

This is similar to what ControlLab does, but integrated into gilrs's existing IOKit event loop.

### Files changed

- `gilrs-core/src/platform/macos/io_kit.rs` (element classification)
- `gilrs-core/src/platform/macos/gamepad.rs` (vendor element handling, native_ev_codes)

---

## Phase 4 — Windows: WGI / RawGameController path

### Strategy

Two scenarios:

1. **WgiGamepad match:** Controller mapped to standard Xbox gamepad → extra buttons invisible. Need Windows HID API.
2. **RawGameController fallback:** Extra buttons MAY appear as raw buttons.

### Step 4a — Check what WGI exposes (requires hardware)

```rust
// Does WgiGamepad::FromGameController succeed?
// If not, what is RawGameController::ButtonCount()?
```

### Step 4b — If RawGameController sees extra buttons

Map the extra button indices to M1-M4, C, Z based on position. Add VID/PID detection:

```rust
if detect::is_vader4_pro(vid, pid) && !is_wgi_gamepad {
    // Raw button indices 14+ are likely M1-M4, C, Z
    // Map them to native_ev_codes for the new Button variants
}
```

### Step 4c — If WgiGamepad hides extra buttons: Windows HID fallback

Use the `windows` crate's HID APIs:

```rust
// HidD_GetHidGuid → SetupDiGetClassDevs → filter by VID/PID
// CreateFile (non-exclusive) → ReadFile for reports
// Decode with protocol_v1::decode_extra_buttons()
```

Since V1 needs no init, this is read-only. Open the HID device, read reports in a background thread, inject events.

### Files changed

- `gilrs-core/src/platform/windows_wgi/gamepad.rs` (detection, raw button mapping)
- `gilrs-core/src/platform/windows_wgi/flydigi_hid.rs` (NEW, only if WGI hides buttons)

---

## Phase 5 — Tests

### Protocol decoder tests (Phase 1)

- Every V1 bit pattern for byte 7
- Edge detection sequences
- Invalid inputs (short, wrong report ID)
- All 64 combinations of 6 buttons

### Integration tests

- Synthetic Vader 4 Pro reports → gilrs events
- Verify `Button::C`, `Button::Z`, `Button::M1`..`M4` emitted correctly
- Verify simultaneous presses work
- Verify non-Flydigi controllers unaffected

### Regression tests

- Full existing gilrs test suite passes
- Xbox, PlayStation, Nintendo controllers unchanged

---

## Phase 6 — Documentation

- Supported controller matrix (Vader 4 Pro: USB, 2.4G receiver)
- Linux udev rule for hidraw (if needed)
- Bluetooth status: UNKNOWN — document as untested
- Button names: C, Z, M1, M2, M3, M4

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Kernel doesn't expose V1 extra buttons as evdev | Phase 2 evdev path fails | hidraw fallback (Step 2c) |
| IOKit doesn't enumerate vendor elements | Phase 3 element path fails | Raw report callback fallback (Step 3d) |
| WGI maps to WgiGamepad hiding extras | Phase 4 simple path fails | Windows HID fallback (Step 4c) |
| hidraw permission denied on Linux | Extra buttons unavailable | Document udev rule; graceful fallback |
| Firmware revisions have different byte layouts | Wrong buttons | Detect device ID; test multiple versions |
| C/Z not present on all V4 Pro revisions | False positives | SDL has_cz flag; per-revision capability |

---

## Priority Order

1. **Phase 0 + Phase 1** — Button API + protocol decoder (no hardware needed)
2. **Phase 2** — Linux evdev (needs hardware to verify kernel behavior)
3. **Phase 3** — macOS IOKit (needs hardware to verify element enumeration)
4. **Phase 4** — Windows WGI (needs hardware to verify button visibility)
5. **Phase 5 + 6** — Tests + docs

Phases 2, 3, 4 are independent per-platform and can run in parallel once Phase 1 is done.
---

## Implementation notes (what was actually built)

Deviations from the plan above, all deliberate:

1. **VID/PID.** Only `0x04B4:0x2412` is used for device detection. The plan lists
   `0x37D7:0x3001` as well, but that combination is not present in SDL3's `usb_ids.h`,
   in the Linux kernel's `hid-ids.h`, or in any other examined implementation, so it is
   treated as UNVERIFIED and is not enabled. It is kept as a documented constant
   (`gilrs_core::flydigi::detect::VADER4_PRO_PRODUCT_ID`).
2. **Report validation.** `protocol_v1` also requires `report[1] == 0xFE`, as SDL3
   does, instead of only checking the report ID. This makes it impossible for an
   unrelated `0x04` report to be misread as an input report.
3. **Button API.** `Button` additionally gets `#[non_exhaustive]` so that a future
   extra button does not break downstream `match` statements again.
4. **Extra button mapping.** The macro buttons are not added to
   `Mapping::default()`; a separate `Mapping::add_extra_buttons` is applied to *every*
   base mapping. Reason: an SDL mapping from the bundled database exists for some
   Flydigi devices (`03000000b404000012240000...`), and it would otherwise shadow the
   extra buttons and report them as `Button::Unknown`. The helper is a no-op unless the
   gamepad exposes the vendor macro codes.
5. **`C`/`Z`.** They reuse the existing `BTN_C`/`BTN_Z` native codes rather than
   getting new ones; they are only synthesized from the vendor report when the kernel
   does not expose them, so no duplicate events can be produced.
6. **Linux transport.** The vendor report is read from the `/dev/hidraw*` node that
   belongs to the same HID device (with a same-USB-device fallback for revisions that
   put the vendor report on a different interface). The fd is registered in the same
   `epoll` as the evdev fd, and it is removed from the interest list if a read fails so
   that a hung up fd cannot make the event loop spin.
7. **macOS / Windows (Phases 3 and 4) are not implemented.** Only the platform
   independent parts (Button API, `native_ev_codes`, decoder, mapping) are in place;
   there was no hardware to verify an IOKit or Windows HID transport against, and
   shipping an unverifiable transport was considered worse than not shipping one. The
   decoder is transport agnostic, so a transport only has to produce `&[u8]`.
8. **V2 (Vader 5 Pro) is not implemented.** It needs a separate interface, an
   initialization handshake and a different report layout; see `docs/flydigi.md`.
