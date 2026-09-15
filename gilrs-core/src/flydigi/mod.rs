// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Flydigi vendor HID support.
//!
//! Flydigi "Vader" (and "Apex") controllers hide several physical buttons from the
//! standard gamepad stack. Depending on the model and connection mode these buttons
//! are only present in a vendor specific HID report (or in vendor specific bytes of a
//! HID report that the OS level gamepad driver ignores):
//!
//! * `C`, `Z` - extra face buttons
//! * `M1`-`M4` - extra macro/back buttons
//!
//! This module contains the platform independent parts of the support:
//!
//! * [`detect`] - VID/PID based device identification and capabilities.
//! * [`protocol_v1`] - decoder for the first generation vendor protocol, used by the
//!   Vader 2/3/4 Pro (and Apex 2/3/4) when connected in DInput mode.
//! * [`types`] - shared types (button sets, decoded input, decode errors).
//!
//! OS specific code only has to acquire raw reports and feed them to a
//! [`types::ReportDecoder`]. See each platform backend for the transport details.
//!
//! # Status
//!
//! Only the `V1` protocol is implemented. The `V2` protocol (Vader 5 Pro / Apex 5)
//! uses a separate vendor HID interface (`0xFFA0`) that requires an initialization
//! handshake before it emits reports; see `docs/flydigi.md` for the collected
//! reverse engineering notes and the list of open questions.

pub mod detect;
pub mod protocol_v1;
pub mod types;

pub use self::detect::{identify, Capabilities, DeviceInfo, Model, Protocol};
pub use self::protocol_v1::Vader4ProDecoder;
pub use self::protocol_v1::{decode_extra_buttons, REPORT_ID, REPORT_LEN};
pub use self::types::{
    ButtonEvent, ButtonState, DecodeError, FlydigiButton, FlydigiButtons, FlydigiInput,
    ReportDecoder,
};
