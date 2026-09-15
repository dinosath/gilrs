// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Flydigi device identification.
//!
//! Everything here is based on observed USB descriptors and on the reverse
//! engineering work listed in `docs/flydigi.md`. Claims are marked as
//! `CONFIRMED`, `INFERRED` or `UNVERIFIED`, following the same convention as the
//! research document. Never turn an `UNVERIFIED` entry into a hard-coded mapping
//! without documenting it - that is why [`identify`] only returns a device for the
//! `CONFIRMED` combinations and [`DeviceInfo::verified`] tells callers how much the
//! identification can be trusted.

/// Flydigi's own USB vendor ID.
///
/// Used by the Vader 5 Pro / Apex 5 2.4 GHz receiver (`37D7:2401`, `37D7:2501`).
/// CONFIRMED - SDL3 `usb_ids.h` `USB_VENDOR_FLYDIGI_V2`.
pub const FLYDIGI_VENDOR_ID: u16 = 0x37d7;

/// Cypress Semiconductor vendor ID.
///
/// Flydigi used a Cypress based wireless receiver for the first protocol
/// generation. CONFIRMED - SDL3 `usb_ids.h` `USB_VENDOR_FLYDIGI_V1`.
pub const CYPRESS_VENDOR_ID: u16 = 0x04b4;

/// Vader 5 Pro (and Vader 4 Pro in DInput mode) vendor product ID behind the
/// Cypress receiver. CONFIRMED - SDL3 `USB_PRODUCT_FLYDIGI_V1_GAMEPAD` and
/// `dantmnf/Vader4ProReader` (`HID\VID_04B4&PID_2412&MI_02`).
pub const VADER4_PRO_DINPUT_PRODUCT_ID: u16 = 0x2412;

/// Vader 4 Pro claiming the Flydigi vendor ID directly.
///
/// UNVERIFIED. This combination is not present in SDL3, the Linux kernel
/// `hid-ids.h` or any of the other examined implementations; it is only mentioned
/// in the second hand notes of the research document. It is therefore *not* part of
/// [`identify`] and remains here only so that the open question is recorded in code.
pub const VADER4_PRO_PRODUCT_ID: u16 = 0x3001;

/// Vader 5 Pro 2.4 GHz receiver. CONFIRMED - SDL3 `USB_PRODUCT_FLYDIGI_V2_VADER`.
pub const VADER5_PRO_PRODUCT_ID: u16 = 0x2401;

/// Apex 5 2.4 GHz receiver. CONFIRMED - SDL3 `USB_PRODUCT_FLYDIGI_V2_APEX`.
pub const APEX5_PRODUCT_ID: u16 = 0x2501;

/// Vendor HID usage page used by the second generation Flydigi devices.
///
/// CONFIRMED - SDL3 `USB_USAGEPAGE_VENDOR_FLYDIGI`, ControlLab, `vader5pro-hid-tools`.
pub const VENDOR_USAGE_PAGE: u32 = 0xffa0;

/// Vendor protocol generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Numbered reports (`Report ID 0x04`), no initialization required, extra
    /// buttons in the same report as the standard buttons.
    V1,
    /// `5A A5` framed reports on a separate vendor HID interface. Requires an
    /// initialization handshake before it emits input reports.
    V2,
}

/// Model family as far as it can be determined from USB IDs alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    /// Vader 2/2 Pro/3/3 Pro/4 Pro (and Apex 2/3/4). The actual model is only
    /// known after the `0xEC` info query, which this crate does not perform.
    Vader4Family,
    /// Vader 5 Pro.
    Vader5Pro,
    /// Apex 5.
    Apex5,
    /// A Flydigi device that could not be identified more precisely.
    Unknown,
}

/// Which extra buttons the device is known to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Capabilities {
    pub c: bool,
    pub z: bool,
    pub m1: bool,
    pub m2: bool,
    pub m3: bool,
    pub m4: bool,
}

impl Capabilities {
    /// No extra buttons.
    pub const NONE: Capabilities = Capabilities {
        c: false,
        z: false,
        m1: false,
        m2: false,
        m3: false,
        m4: false,
    };

    /// `C`, `Z` and `M1`-`M4`, as exposed by the V1 protocol.
    pub const CZ_M1_M4: Capabilities = Capabilities {
        c: true,
        z: true,
        m1: true,
        m2: true,
        m3: true,
        m4: true,
    };
}

/// Identification result for a USB VID/PID pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub model: Model,
    pub protocol: Protocol,
    pub capabilities: Capabilities,
    /// `true` when the VID/PID pair is confirmed by an independent implementation.
    pub verified: bool,
}

impl DeviceInfo {
    /// Returns the decoder that should be used for reports from this device.
    pub fn decoder(self) -> crate::flydigi::Vader4ProDecoder {
        crate::flydigi::Vader4ProDecoder
    }
}

/// Identifies a Flydigi controller from its USB IDs.
///
/// Only confirmed combinations return `Some` - see the constants for the
/// confidence of each entry. `None` means "this is not a known Flydigi device",
/// which is the answer callers must handle by doing nothing at all.
pub fn identify(vendor_id: u16, product_id: u16) -> Option<DeviceInfo> {
    match (vendor_id, product_id) {
        // CONFIRMED: Cypress receiver, V1 protocol, DInput mode.
        (CYPRESS_VENDOR_ID, VADER4_PRO_DINPUT_PRODUCT_ID) => Some(DeviceInfo {
            vendor_id,
            product_id,
            model: Model::Vader4Family,
            protocol: Protocol::V1,
            capabilities: Capabilities::CZ_M1_M4,
            verified: true,
        }),
        // CONFIRMED: Vader 5 Pro 2.4 GHz receiver. The vendor HID interface is a
        // separate interface and needs initialization, so no decoder is wired up
        // yet - but the device is recognized so callers can report it.
        (FLYDIGI_VENDOR_ID, VADER5_PRO_PRODUCT_ID) => Some(DeviceInfo {
            vendor_id,
            product_id,
            model: Model::Vader5Pro,
            protocol: Protocol::V2,
            capabilities: Capabilities::CZ_M1_M4,
            verified: true,
        }),
        // CONFIRMED: Apex 5 2.4 GHz receiver.
        (FLYDIGI_VENDOR_ID, APEX5_PRODUCT_ID) => Some(DeviceInfo {
            vendor_id,
            product_id,
            model: Model::Apex5,
            protocol: Protocol::V2,
            capabilities: Capabilities::CZ_M1_M4,
            verified: true,
        }),
        _ => None,
    }
}

/// Returns `true` if `vendor_id`/`product_id` is a confirmed V1 Flydigi device.
pub fn is_vader4_pro(vendor_id: u16, product_id: u16) -> bool {
    identify(vendor_id, product_id)
        .map(|info| info.protocol == Protocol::V1)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_vader4_pro_is_recognized() {
        let info = identify(CYPRESS_VENDOR_ID, VADER4_PRO_DINPUT_PRODUCT_ID).unwrap();
        assert_eq!(info.model, Model::Vader4Family);
        assert_eq!(info.protocol, Protocol::V1);
        assert!(info.verified);
        assert_eq!(info.capabilities, Capabilities::CZ_M1_M4);
        assert!(is_vader4_pro(
            CYPRESS_VENDOR_ID,
            VADER4_PRO_DINPUT_PRODUCT_ID
        ));
    }

    #[test]
    fn unverified_vader4_pro_pid_is_not_activated() {
        assert_eq!(identify(FLYDIGI_VENDOR_ID, VADER4_PRO_PRODUCT_ID), None);
        assert!(!is_vader4_pro(FLYDIGI_VENDOR_ID, VADER4_PRO_PRODUCT_ID));
    }

    #[test]
    fn vader5_pro_is_recognized_but_uses_v2() {
        let info = identify(FLYDIGI_VENDOR_ID, VADER5_PRO_PRODUCT_ID).unwrap();
        assert_eq!(info.model, Model::Vader5Pro);
        assert_eq!(info.protocol, Protocol::V2);
        assert!(!is_vader4_pro(FLYDIGI_VENDOR_ID, VADER5_PRO_PRODUCT_ID));
    }

    #[test]
    fn unrelated_devices_are_rejected() {
        assert_eq!(identify(0x045e, 0x028e), None); // Xbox 360 / XInput
        assert_eq!(identify(0x054c, 0x09cc), None); // DualShock 4
        assert_eq!(identify(0x0f0d, 0x00c1), None); // HORIPAD
        assert_eq!(identify(CYPRESS_VENDOR_ID, 0x0000), None);
        assert_eq!(identify(FLYDIGI_VENDOR_ID, 0x0000), None);
    }
}
