# Flydigi support in gilrs

Flydigi "Vader" controllers have a number of physical buttons that the operating
system's gamepad stack does not report:

| Button | Location | Notes |
|--------|----------|-------|
| `C` | extra face button | present on the Vader series |
| `Z` | extra face button | present on the Vader series |
| `M1`-`M4` | extra macro/back buttons | present on the Vader series |

gilrs exposes them as first class buttons:

```rust
use gilrs::{Button, EventType, Gilrs};

let mut gilrs = Gilrs::new().unwrap();

while let Some(ev) = gilrs.next_event() {
    match ev.event {
        EventType::ButtonPressed(Button::M1, _) => println!("M1"),
        EventType::ButtonPressed(Button::M2, _) => println!("M2"),
        EventType::ButtonPressed(Button::M3, _) => println!("M3"),
        EventType::ButtonPressed(Button::M4, _) => println!("M4"),
        EventType::ButtonPressed(Button::C, _) => println!("C"),
        EventType::ButtonPressed(Button::Z, _) => println!("Z"),
        _ => {}
    }
}
```

No Flydigi SpaceStation, remapper, keyboard emulation or Steam Input is involved.
`M1` is reported as `Button::M1` - it is **never** silently mapped to `LeftTrigger`,
`RightTrigger` or any other standard button.

## Supported controllers and transports

| Controller | Protocol | USB | 2.4G receiver | Bluetooth | C | Z | M1 | M2 | M3 | M4 |
|------------|----------|-----|---------------|-----------|---|---|----|----|----|----|
| Vader 4 Pro / Vader 3 Pro / Vader 2 Pro (DInput mode) | V1 | portable* | portable* | unknown | yes | yes | yes | yes | yes | yes |
| Vader 5 Pro | V2 | n/a (receiver only) | not implemented | no | yes | yes | yes | yes | yes | yes |

\* "portable" means the protocol decoder is platform independent but the *transport*
is only implemented on Linux so far; see [Status](#status) below.

`M5` and `M6` do not exist on any controller examined for this implementation - see
[M5/M6](#m5m6).

### What "V1" means

Two generations of the Flydigi vendor protocol were found:

* **V1** - numbered HID reports (`Report ID 0x04`) with the extra buttons in the same
  report and on the same interface as the standard buttons. No initialization is
  required; reports flow as soon as the device is opened. Used by the Vader 2/3/4 Pro
  and the Apex 2/3/4.
* **V2** - `5A A5` framed reports on a *separate* vendor HID interface (usage page
  `0xFFA0`). Requires an initialization handshake (`5A A5 11 07 FF 01 FF FF FF 15`)
  before it emits `5A A5 EF` input reports. Used by the Vader 5 Pro and Apex 5.

Only V1 is implemented. V2 devices are recognized (so that they can be reported), but
no events are generated for them yet.

### XInput vs DInput

The extra buttons only exist in DInput mode. In XInput mode the controller does not
send them at all - if Space Station has remapped them, the controller replays the
mapped buttons instead. This is not something gilrs can work around:

> X-Input mode does not support the custom buttons as native hardware events, they
> will replay the bindings mapped in the Flydigi SpaceStation app.
> - `ahungry/vader3`, README

## Linux

On Linux the kernel's `hid-generic` driver creates the evdev node that gilrs already
uses, but it does not understand the vendor defined extra button byte, so `C`, `Z` and
`M1`-`M4` never appear on `/dev/input/event*`.

The raw reports are still available on `/dev/hidraw*` for the *same* HID device.
Reading hidraw does not consume reports from the input subsystem (both are fed from the
same HID driver), so this cannot break normal gamepad input. gilrs opens that node
read-only and non-blocking next to the evdev fd and feeds the reports to the protocol
decoder.

### Permissions

`/dev/hidraw*` is usually root-only. Install a udev rule, for example
[`contrib/udev/70-gilrs-flydigi.rules`](../contrib/udev/70-gilrs-flydigi.rules):

```udev
# Cypress based Flydigi receiver (Vader 2/3/4 Pro, Apex 2/3/4) in DInput mode
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="04b4", ATTRS{idProduct}=="2412", TAG+="uaccess"
```

Copy it to `/etc/udev/rules.d/` and run `sudo udevadm control --reload && sudo udevadm
trigger`. Users without access simply do not get the extra buttons - nothing else
changes.

## Status

| Item | State |
|------|-------|
| Protocol decoder (`gilrs-core/src/flydigi/`) | implemented, unit tested |
| `Button::M1`-`M4` public API | implemented |
| Mapping integration | implemented |
| Linux hidraw transport | implemented, **not tested on hardware** |
| macOS transport | **not implemented** |
| Windows transport | **not implemented** |
| V2 (Vader 5 Pro) decoder | **not implemented** |
| Bluetooth | **not supported** (see below) |

### Validation report

Physical Vader hardware was not available while this was implemented, so every
hardware dependent statement below is marked `UNTESTED`. Nothing is claimed to work on
hardware that has not actually been run.

| Controller | Transport | Detection | Extra buttons | Standard buttons unaffected |
|------------|-----------|-----------|---------------|------------------------------|
| Vader 4 Pro | USB (DInput) | UNTESTED | UNTESTED | UNTESTED |
| Vader 4 Pro | 2.4G receiver (DInput) | UNTESTED | UNTESTED | UNTESTED |
| Vader 4 Pro | Bluetooth | UNKNOWN | UNKNOWN | UNKNOWN |
| Vader 5 Pro | 2.4G receiver | UNTESTED | UNSUPPORTED (V2 not implemented) | UNTESTED |
| Vader 5 Pro | Bluetooth | n/a | NO (Xbox BT profile has no vendor report) | n/a |
| Any non-Flydigi gamepad | any | n/a | NO | tested (unit tests) |

What *is* tested without hardware:

* every bit pattern of the extra button byte decodes to the documented button;
* `Pressed`/`Released` edge detection, including simultaneous changes and holds;
* short/long/wrong-report-ID reports are rejected without panicking;
* an unrelated gamepad never gets `M1`-`M4` (`gilrs/src/mapping/mod.rs` tests);
* SDL's `paddle*` tokens do not become macro buttons;
* the crate builds for Linux, Windows (WGI and XInput), macOS and wasm32.

## Protocol reference (V1)

Everything in this section is `CONFIRMED` by at least two independent
implementations unless stated otherwise.

| Property | Value | Evidence |
|----------|-------|----------|
| VID:PID | `0x04B4:0x2412` | SDL3 `usb_ids.h`, `dantmnf/Vader4ProReader` (`HID\VID_04B4&PID_2412&MI_02`) |
| Report ID | `0x04` | SDL3 `HIDAPI_DriverFlydigi_HandlePacketV1`, Vader4ProReader |
| Report marker | byte 1 == `0xFE` | SDL3 (rejects anything else) |
| Report size | 32 bytes | all sources |
| Extra buttons | byte 7 | SDL3, Vader4ProReader, `BANANASJIM/flydigi-vader5` |
| Initialization | none required | SDL3 V1 driver never sends an enable command |
| Interface | same HID interface as the gamepad (IF0/IF2 depending on revision) | SDL3 (`interface_number == 2` for early controllers), `BANANASJIM/flydigi-vader5` |

### Byte 7 bit layout

| Bit | Mask | Button | Confidence |
|-----|------|--------|------------|
| 0 | `0x01` | `C` | CONFIRMED (SDL3 `has_cz`, Vader4ProReader) |
| 1 | `0x02` | `Z` | CONFIRMED (SDL3 `has_cz`, Vader4ProReader) |
| 2 | `0x04` | `M1` | CONFIRMED (all sources) |
| 3 | `0x08` | `M2` | CONFIRMED (all sources) |
| 4 | `0x10` | `M3` | CONFIRMED (all sources) |
| 5 | `0x20` | `M4` | CONFIRMED (all sources) |
| 6 | `0x40` | - | UNKNOWN on V1 (LM on V2) |
| 7 | `0x80` | - | UNKNOWN on V1 (RM on V2) |

Bits 6 and 7 are ignored on purpose: a future revision that starts using them cannot
produce phantom button presses.

### Other bytes (not used by this implementation)

| Offset | Content |
|--------|---------|
| 0 | report ID (`0x04`) |
| 1 | `0xFE` marker |
| 2 | unknown (observed `0x66`) |
| 3 | air mouse active flag (`0x80`) |
| 4-6 | legacy motion data |
| 7 | extra buttons (see above) |
| 8 | system buttons: `0x01` Fn/'+', `0x08` Home |
| 9 | D-Pad (low nibble) + A/B/Select/X |
| 10 | Y/Start/LB/RB/LT/RT/LS/RS |
| 11-16 | accelerometer + sticks |
| 17-30 | sticks, triggers, gyroscope |
| 31 | reserved |

Only offset 7 is decoded by gilrs; the standard buttons, sticks and triggers keep
coming from evdev as before.

## M5/M6

No evidence of `M5`/`M6` buttons exists in any examined source:

* SDL3's Flydigi driver only knows `M1`-`M4` plus `LM`/`RM` (which SDL reports as
  `misc4`/`misc5`, not as `M5`/`M6`);
* `BANANASJIM/flydigi-vader5` (`ExtButton`) has `M1`-`M4` and `LM`/`RM`;
* `dantmnf/Vader4ProReader` has `M1`-`M4`;
* ControlLab lists `M1-M4`, `LM/RM`, `Home`, `Fn/O`;
* `DrProton824/vader5pro-hid-tools` lists `M1`-`M4`, `LM/RM`.

The naming in the assignment ("M5/M6") most likely refers to `LM`/`RM`, which sit on
byte 13 bits 6/7 of the V2 report. They are **not** exposed by this implementation,
because gilrs has no suitable `Button` for them and no hardware was available to
confirm their behaviour. They are listed here so the open question is not lost.

## Bluetooth

`CONFIRMED`: the Vader 5 Pro's Bluetooth profile is a standard Xbox-compatible
gamepad and does not carry the vendor report. ControlLab documents this explicitly
("Vader-specific extra buttons, motion sensors, and firmware metadata are not present
in its Bluetooth report; use the USB receiver for those features").

For the Vader 4 Pro, the Bluetooth report layout is `UNKNOWN` - no source describing
it was found. Since the vendor buttons are only produced in DInput mode and the
handful of community drivers all target a wired/dongle connection, support is
deliberately not claimed.

The transport is abstracted (`FlydigiReader` on Linux); adding a Bluetooth transport
would mean adding another transport that feeds the same decoder, not redesigning the
feature.

## Diagnosing your device

```sh
# 1. Is the controller visible at all?
lsusb | grep -i -E '37d7|04b4'
#    -> 04b4:2412 is a V1 device (supported), 37d7:2401 is a V2 device (not yet).

# 2. Did the kernel create an evdev gamepad?
sudo evtest

# 3. Which hidraw node belongs to it?
ls -l /sys/class/hidraw/
cat /sys/class/hidraw/hidraw*/device/uevent | grep HID_ID

# 4. Can you read it? (needs the udev rule or root)
sudo cat /dev/hidrawN | xxd | head
#    -> a report starting with 04 fe is the V1 vendor input report.
```

Check that gilrs sees the device:

```sh
cargo run --example gamepad_info

# and that the extra buttons are decoded
cargo run --example flydigi
```

Set `RUST_LOG=gilrs_core=info` to see the messages emitted when the vendor reader is
attached.

## Capturing a report for an unsupported revision

If your controller is recognized but the extra buttons do not work, or it is not
recognized at all, a raw capture is what is needed to add support:

```sh
# hidraw capture (simplest)
sudo cat /dev/hidrawN | xxd > capture.txt

# or a USB capture for interface/endpoint information
sudo modprobe usbmon
sudo tshark -i usbmon1 -Y 'usb.device_address == <n>' > capture.txt
```

Press one extra button at a time, note the order, and include the `lsusb` output, the
HID report descriptor (`sudo cat /sys/class/hidraw/hidrawN/device/report_descriptor |
xxd`) and the captured reports in the issue. That is enough to identify the report ID,
the byte offset and the bit masks.

## Implementation

```
gilrs-core/src/flydigi/
    mod.rs          module root and re-exports
    detect.rs       VID/PID -> model, protocol, capabilities
    types.rs        button sets, edge detection, ReportDecoder trait
    protocol_v1.rs  V1 report decoder (pure, no OS dependencies)
gilrs-core/src/platform/linux/flydigi_hid.rs
                    hidraw transport: find the node, read, decode, emit
gilrs/src/ev/mod.rs         Button::C/Z/M1-M4, is_macro(), to_nec()
gilrs/src/mapping/mod.rs    extra button mapping, applied on top of any base mapping
```

The decoder is platform independent: it operates on `&[u8]` and has no OS specific
code, so a macOS or Windows transport only has to obtain raw reports and call
`Vader4ProDecoder::decode`.

## Sources

* SDL3 `src/joystick/hidapi/SDL_hidapi_flydigi.c` and `src/joystick/usb_ids.h`
* `dantmnf/Vader4ProReader` - `Device/Vader4ProReport.cs`
* `BANANASJIM/flydigi-vader5` - `docs/protocol.md`, `include/vader5/*.hpp`
* `dracinn/ControlLab` - `Sources/Vader5Core/Vader5Protocol.swift`
* `DrProton824/vader5pro-hid-tools` - `docs/*ReverseEngineering.md`
* `ahungry/vader3`, `zeptic99/flydigiv4pro-linux`
* SDL `gamecontrollerdb.txt`
