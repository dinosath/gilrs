// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
#![allow(unused_variables)]

use std::time::Duration;

/// Represents gamepad. Reexported as FfDevice
///
/// Rumble on iOS runs through `GCDeviceHaptics` and CoreHaptics rather than a
/// pair of motor magnitudes, so this is a placeholder: `Gamepad::is_ff_supported`
/// reports false and no device is ever handed out.
#[derive(Debug)]
pub struct Device;

impl Device {
    /// Sets magnitude for strong and weak ff motors.
    pub fn set_ff_state(&mut self, strong: u16, weak: u16, min_duration: Duration) {}
}
