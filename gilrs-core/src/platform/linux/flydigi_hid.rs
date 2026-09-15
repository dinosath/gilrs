// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Linux transport for Flydigi vendor HID reports.
//!
//! The `V1` protocol (Vader 2/3/4 Pro, Apex 2/3/4 in DInput mode) puts the extra
//! buttons in the *same* HID report and the *same* USB interface as the standard
//! buttons. The kernel's `hid-generic` driver creates the evdev node that gilrs
//! already uses, but it does not know about the vendor defined extra button byte, so
//! those buttons never reach `/dev/input/event*`.
//!
//! The raw reports are still available through `/dev/hidraw*` for the same HID
//! device. Reading hidraw does **not** consume reports from the input subsystem -
//! both are fed from the same HID driver - so this cannot break the normal gamepad
//! input. This module opens that hidraw node read-only and non-blocking and feeds the
//! reports into [`crate::flydigi::protocol_v1`].
//!
//! Access to `/dev/hidraw*` usually requires root, or a udev rule:
//!
//! ```text
//! SUBSYSTEM=="hidraw", ATTRS{idVendor}=="04b4", ATTRS{idProduct}=="2412", TAG+="uaccess"
//! ```
//!
//! See `docs/flydigi.md`.

use super::gamepad::{native_ev_codes, EvCode};
use crate::flydigi::protocol_v1::Vader4ProDecoder;
use crate::flydigi::types::{
    ButtonEvent, ButtonState, FlydigiButton, FlydigiButtons, ReportDecoder,
};
use crate::flydigi::{DeviceInfo, Protocol};
use crate::utils;

use libc as c;
use nix::errno::Errno;
use nix::sys::epoll::{Epoll, EpollEvent, EpollFlags};

use std::collections::VecDeque;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{BorrowedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Directory that contains the raw HID devices.
const HIDRAW_DIR: &str = "/sys/class/hidraw";

/// Upper bound for the size of a single vendor report. The `V1` report is 32 bytes,
/// but the kernel may hand us a buffer with room to spare.
const MAX_REPORT_LEN: usize = 64;

/// Number of extra buttons the `V1` protocol can report.
const N_BUTTONS: usize = 6;

/// Reads and decodes vendor reports for a single controller.
pub(super) struct FlydigiReader {
    fd: RawFd,
    syspath: PathBuf,
    devpath: String,
    decoder: Vader4ProDecoder,
    state: ButtonState,
    events: VecDeque<(crate::EventType, SystemTime)>,
    /// `EvCode` to use for each entry of [`FlydigiButton::ALL`]. `None` means the
    /// button is already handled by the normal kernel driver and must not be
    /// re-reported by us.
    codes: [Option<EvCode>; N_BUTTONS],
    buffer: [u8; MAX_REPORT_LEN],
    /// Set when the device disappeared. The owner then removes the reader from
    /// `epoll`, so that a hung up fd cannot make the event loop spin.
    failed: bool,
}

impl FlydigiReader {
    /// Tries to attach to the vendor interface of `syspath` (an evdev syspath such
    /// as `/sys/class/input/event12`).
    ///
    /// Returns `None` (and logs at debug level) when the device is not a supported
    /// Flydigi V1 device or when the hidraw node cannot be opened - in that case the
    /// controller simply behaves as before this feature existed.
    pub(super) fn open(
        syspath: &Path,
        info: &DeviceInfo,
        evdev_buttons: &[EvCode],
    ) -> Option<Self> {
        if info.protocol != Protocol::V1 {
            debug!(
                "Flydigi device {:04x}:{:04x} uses the V2 protocol, which is not implemented yet",
                info.vendor_id, info.product_id
            );
            return None;
        }

        let hidraw = match find_hidraw_for_gamepad(syspath, info.vendor_id, info.product_id) {
            Some(hidraw) => hidraw,
            None => {
                debug!(
                    "Could not find the hidraw node of the Flydigi vendor interface for {:?}",
                    syspath
                );
                return None;
            }
        };

        let devpath = hidraw.to_string_lossy().into_owned();
        let cpath = CString::new(hidraw.as_os_str().as_bytes()).ok()?;
        let fd = unsafe { c::open(cpath.as_ptr(), c::O_RDONLY | c::O_NONBLOCK | c::O_CLOEXEC) };
        if fd < 0 {
            debug!(
                "Failed to open {} for reading ({}). A udev rule granting access to \
                 /dev/hidraw* is required for Flydigi extra buttons.",
                devpath,
                Errno::last()
            );
            return None;
        }

        let mut reader = FlydigiReader {
            fd,
            syspath: syspath.to_path_buf(),
            devpath,
            decoder: Vader4ProDecoder,
            state: ButtonState::new(),
            events: VecDeque::new(),
            codes: [None; N_BUTTONS],
            buffer: [0; MAX_REPORT_LEN],
            failed: false,
        };
        reader.codes = Self::select_codes(evdev_buttons);

        info!(
            "Flydigi: reading vendor reports from {} ({:04x}:{:04x})",
            reader.devpath, info.vendor_id, info.product_id
        );

        Some(reader)
    }

    /// Chooses the `EvCode` used for every extra button.
    ///
    /// `C`/`Z` normally have a Linux input code (`BTN_C`/`BTN_Z`) and gilrs already
    /// maps those to `Button::C`/`Button::Z`. They are only synthesized from the
    /// vendor report when the kernel does not expose them, so a device that does
    /// report them through evdev cannot produce duplicated events.
    fn select_codes(evdev_buttons: &[EvCode]) -> [Option<EvCode>; N_BUTTONS] {
        let c_code = if evdev_buttons.contains(&native_ev_codes::BTN_C) {
            None
        } else {
            Some(native_ev_codes::BTN_C)
        };

        let z_code = if evdev_buttons.contains(&native_ev_codes::BTN_Z) {
            None
        } else {
            Some(native_ev_codes::BTN_Z)
        };

        [
            c_code,
            z_code,
            Some(native_ev_codes::BTN_M1),
            Some(native_ev_codes::BTN_M2),
            Some(native_ev_codes::BTN_M3),
            Some(native_ev_codes::BTN_M4),
        ]
    }

    /// All `EvCode`s this reader can produce, in [`FlydigiButton::ALL`] order.
    pub(super) fn codes(&self) -> impl Iterator<Item = EvCode> + '_ {
        self.codes.iter().filter_map(|code| *code)
    }

    /// File descriptor of the hidraw device, for `epoll`.
    pub(super) fn fd(&self) -> RawFd {
        self.fd
    }

    /// `true` if the device went away and the reader should be dropped.
    pub(super) fn is_failed(&self) -> bool {
        self.failed
    }

    /// Returns the next pending event, reading more reports if necessary.
    ///
    /// Reports describe the current button state, so this only produces events for
    /// actual transitions. Never blocks: the fd is opened `O_NONBLOCK`.
    pub(super) fn poll(&mut self) -> Option<(crate::EventType, SystemTime)> {
        if let Some(event) = self.events.pop_front() {
            return Some(event);
        }

        self.read_available();
        self.events.pop_front()
    }

    /// Drains everything the kernel has buffered for us.
    fn read_available(&mut self) {
        loop {
            let n = unsafe {
                c::read(
                    self.fd,
                    self.buffer.as_mut_ptr() as *mut c::c_void,
                    self.buffer.len(),
                )
            };

            if n > 0 {
                let len = n as usize;
                let time = utils::time_now();
                match self.decoder.decode(&self.buffer[..len]) {
                    Ok(input) => self.push_button_state(input.buttons, time),
                    Err(e) => trace!("Ignoring report from {} (len {}): {}", self.devpath, len, e),
                }
            } else if n == 0 {
                // End of file: the device is gone.
                debug!(
                    "{} reached end of file, dropping the vendor reader",
                    self.devpath
                );
                self.failed = true;
                break;
            } else {
                let errno = Errno::last();
                if errno != Errno::EAGAIN && errno != Errno::EWOULDBLOCK {
                    debug!("Failed to read from {}: {}", self.devpath, errno);
                    self.failed = true;
                }
                break;
            }
        }
    }

    /// Converts a decoded button state into gilrs events.
    fn push_button_state(&mut self, buttons: FlydigiButtons, time: SystemTime) {
        for event in self.state.update(buttons) {
            let (button, pressed) = match event {
                ButtonEvent::Pressed(button) => (button, true),
                ButtonEvent::Released(button) => (button, false),
            };

            let code = match self.codes[button_index(button)] {
                Some(code) => code,
                None => continue,
            };

            let event = if pressed {
                crate::EventType::ButtonPressed(crate::EvCode(code))
            } else {
                crate::EventType::ButtonReleased(crate::EvCode(code))
            };

            self.events.push_back((event, time));
        }
    }
}

impl Drop for FlydigiReader {
    fn drop(&mut self) {
        unsafe {
            if self.fd >= 0 {
                c::close(self.fd);
            }
        }
    }
}

impl std::fmt::Debug for FlydigiReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlydigiReader")
            .field("devpath", &self.devpath)
            .field("syspath", &self.syspath)
            .finish()
    }
}

fn button_index(button: FlydigiButton) -> usize {
    FlydigiButton::ALL
        .iter()
        .position(|candidate| *candidate == button)
        .expect("FlydigiButton::ALL contains every button")
}

/// Finds the `/dev/hidrawN` node that belongs to the same HID device as `syspath`.
///
/// The first pass looks for an exact match of the HID device directory, which covers
/// the documented layout (vendor report on the same interface as the gamepad). The
/// second pass accepts any hidraw node of the same physical USB device, because some
/// revisions are reported to put the vendor report on a different interface (0 vs 2)
/// of the same dongle.
fn find_hidraw_for_gamepad(syspath: &Path, vendor_id: u16, product_id: u16) -> Option<PathBuf> {
    // `/sys/class/input/eventN/device` is the input device, its parent is the HID
    // device (`0003:04B4:2412.0005` or similar).
    let hid_device = fs::canonicalize(syspath.join("device/device")).ok()?;
    let usb_device = usb_device_dir(&hid_device);

    let mut fallback = None;

    for entry in fs::read_dir(HIDRAW_DIR).ok()?.flatten() {
        let name = entry.file_name();
        let device = match fs::canonicalize(entry.path().join("device")) {
            Ok(device) => device,
            Err(_) => continue,
        };

        let devnode = Path::new("/dev").join(&name);

        if device == hid_device {
            return Some(devnode);
        }

        if fallback.is_some() {
            continue;
        }

        // Different interface: only accept it when it is the same USB device and its
        // HID descriptor reports the expected vendor/product.
        if usb_device.is_some() && usb_device_dir(&device) == usb_device {
            if let Ok(uevent) = fs::read_to_string(device.join("uevent")) {
                if parse_hid_id(&uevent) == Some((vendor_id, product_id)) {
                    fallback = Some(devnode);
                }
            }
        }
    }

    fallback
}

/// Walks up from a HID device directory to the USB device directory that owns it.
fn usb_device_dir(hid_device: &Path) -> Option<PathBuf> {
    let mut dir = hid_device.to_path_buf();
    // `<usb device>/<interface>/<hid device>` - a handful of levels is plenty.
    for _ in 0..8 {
        if dir.join("idVendor").is_file() && dir.join("idProduct").is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// Parses the `HID_ID` line of a hidraw `uevent` file.
///
/// The value has the form `bus:vendor:product`, with every field as 8 hexadecimal
/// digits, for example `0003:000004B4:00002412`.
fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let value = uevent
        .lines()
        .find_map(|line| line.strip_prefix("HID_ID="))?;
    let mut fields = value.trim().split(':');
    let _bus = fields.next()?;
    let vendor = fields.next()?;
    let product = fields.next()?;
    if fields.next().is_some() {
        return None;
    }

    Some((
        u16::from_str_radix(vendor, 16).ok()?,
        u16::from_str_radix(product, 16).ok()?,
    ))
}

/// Registers `reader` in `epoll` with the same payload as the gamepad.
pub(super) fn register(reader: &FlydigiReader, epoll: &Epoll, data: u64) -> Result<(), Errno> {
    let fd = unsafe { BorrowedFd::borrow_raw(reader.fd()) };
    epoll.add(fd, EpollEvent::new(EpollFlags::EPOLLIN, data))
}

/// Removes `reader` from `epoll`.
pub(super) fn unregister(reader: &FlydigiReader, epoll: &Epoll) {
    let fd = unsafe { BorrowedFd::borrow_raw(reader.fd()) };
    if let Err(e) = epoll.delete(fd) {
        debug!("Failed to remove Flydigi hidraw fd from epoll: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flydigi::protocol_v1::{EXTRA_BUTTONS_OFFSET, REPORT_ID, REPORT_LEN};
    use crate::flydigi::types::FlydigiButton;

    #[test]
    fn parses_hid_id() {
        let uevent = "DRIVER=hid-generic\nHID_ID=0003:000004B4:00002412\nHID_NAME=Flydigi \
                      VADER4\nHID_PHYS=usb-0000:00:14.0-3/input2\n";
        assert_eq!(parse_hid_id(uevent), Some((0x04b4, 0x2412)));
    }

    #[test]
    fn rejects_malformed_hid_id() {
        assert_eq!(parse_hid_id(""), None);
        assert_eq!(parse_hid_id("DRIVER=hid-generic\n"), None);
        assert_eq!(parse_hid_id("HID_ID=0003:000004B4\n"), None);
        assert_eq!(parse_hid_id("HID_ID=0003:zzzz:00002412\n"), None);
        assert_eq!(parse_hid_id("HID_ID=0003:000004B4:00002412:extra\n"), None);
    }

    #[test]
    fn vendor_codes_are_distinct_and_not_evdev_codes() {
        let codes = [
            (FlydigiButton::M1, native_ev_codes::BTN_M1),
            (FlydigiButton::M2, native_ev_codes::BTN_M2),
            (FlydigiButton::M3, native_ev_codes::BTN_M3),
            (FlydigiButton::M4, native_ev_codes::BTN_M4),
        ];

        for (button, code) in codes {
            assert!(code.is_vendor(), "{button} is not a vendor code");
        }

        for (i, (_, a)) in codes.iter().enumerate() {
            for (_, b) in &codes[i + 1..] {
                assert_ne!(a, b);
            }
        }

        // The synthetic kind must not collide with a real Linux event type.
        assert!(native_ev_codes::BTN_M1.is_vendor());
    }

    #[test]
    fn c_and_z_are_not_synthesized_when_evdev_exposes_them() {
        let exposed = [
            native_ev_codes::BTN_C,
            native_ev_codes::BTN_Z,
            native_ev_codes::BTN_SOUTH,
        ];
        let codes = FlydigiReader::select_codes(&exposed);
        assert_eq!(codes[0], None);
        assert_eq!(codes[1], None);
        assert_eq!(codes[2], Some(native_ev_codes::BTN_M1));

        let codes = FlydigiReader::select_codes(&[native_ev_codes::BTN_SOUTH]);
        assert_eq!(codes[0], Some(native_ev_codes::BTN_C));
        assert_eq!(codes[1], Some(native_ev_codes::BTN_Z));
    }

    /// End to end test of the decode path without hardware: feed synthetic reports
    /// into the reader and check the emitted gilrs events.
    #[test]
    fn decodes_synthetic_reports_into_events() {
        let mut reader = SyntheticReader::new(&[]);

        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        report[1] = 0xfe;
        report[EXTRA_BUTTONS_OFFSET] = 0x00;

        assert!(reader.push(&report).is_empty());

        // M1 + M2 pressed in one report.
        report[EXTRA_BUTTONS_OFFSET] = 0x0c;
        let events = reader.push(&report);
        assert_eq!(
            events,
            vec![
                crate::EventType::ButtonPressed(crate::EvCode(native_ev_codes::BTN_M1)),
                crate::EventType::ButtonPressed(crate::EvCode(native_ev_codes::BTN_M2)),
            ]
        );

        // Holding them must not repeat the events.
        assert!(reader.push(&report).is_empty());

        // Add C and release M1.
        report[EXTRA_BUTTONS_OFFSET] = 0x08 | 0x01;
        let events = reader.push(&report);
        assert_eq!(
            events,
            vec![
                crate::EventType::ButtonPressed(crate::EvCode(native_ev_codes::BTN_C)),
                crate::EventType::ButtonReleased(crate::EvCode(native_ev_codes::BTN_M1)),
            ]
        );

        // Release everything.
        report[EXTRA_BUTTONS_OFFSET] = 0x00;
        let events = reader.push(&report);
        assert_eq!(
            events,
            vec![
                crate::EventType::ButtonReleased(crate::EvCode(native_ev_codes::BTN_C)),
                crate::EventType::ButtonReleased(crate::EvCode(native_ev_codes::BTN_M2)),
            ]
        );
    }

    #[test]
    fn ignores_unrelated_reports() {
        let mut reader = SyntheticReader::new(&[]);

        // Flydigi command report (report ID 0x05) and the V2 idle packet.
        let mut command = [0u8; REPORT_LEN];
        command[0] = 0x05;
        command[1] = 0xec;
        assert!(reader.push(&command).is_empty());

        let mut idle = [0u8; REPORT_LEN];
        idle[0] = 0x5a;
        idle[1] = 0xa5;
        idle[2] = 0xef;
        assert!(reader.push(&idle).is_empty());
    }

    /// Test double for [`FlydigiReader`] that does not need a real file descriptor.
    struct SyntheticReader {
        state: ButtonState,
        decoder: Vader4ProDecoder,
        codes: [Option<EvCode>; N_BUTTONS],
    }

    impl SyntheticReader {
        fn new(evdev_buttons: &[EvCode]) -> Self {
            SyntheticReader {
                state: ButtonState::new(),
                decoder: Vader4ProDecoder,
                codes: FlydigiReader::select_codes(evdev_buttons),
            }
        }

        fn push(&mut self, report: &[u8]) -> Vec<crate::EventType> {
            let buttons = match self.decoder.decode(report) {
                Ok(input) => input.buttons,
                Err(_) => return Vec::new(),
            };

            let mut events = Vec::new();
            for event in self.state.update(buttons) {
                let (button, pressed) = match event {
                    ButtonEvent::Pressed(button) => (button, true),
                    ButtonEvent::Released(button) => (button, false),
                };
                if let Some(code) = self.codes[button_index(button)] {
                    events.push(if pressed {
                        crate::EventType::ButtonPressed(crate::EvCode(code))
                    } else {
                        crate::EventType::ButtonReleased(crate::EvCode(code))
                    });
                }
            }
            events
        }
    }
}
