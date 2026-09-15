// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Types shared by the Flydigi protocol decoders.

use std::fmt::{Display, Formatter, Result as FmtResult};

/// Number of extra buttons supported by the `V1` protocol.
pub const FLYDIGI_BUTTON_COUNT: usize = 6;

/// A single extra button of a Flydigi controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlydigiButton {
    C,
    Z,
    M1,
    M2,
    M3,
    M4,
}

impl FlydigiButton {
    /// All buttons, in bit order.
    pub const ALL: [FlydigiButton; FLYDIGI_BUTTON_COUNT] = [
        FlydigiButton::C,
        FlydigiButton::Z,
        FlydigiButton::M1,
        FlydigiButton::M2,
        FlydigiButton::M3,
        FlydigiButton::M4,
    ];

    /// Bit mask of this button inside the extra button byte.
    pub const fn mask(self) -> u8 {
        match self {
            FlydigiButton::C => 0x01,
            FlydigiButton::Z => 0x02,
            FlydigiButton::M1 => 0x04,
            FlydigiButton::M2 => 0x08,
            FlydigiButton::M3 => 0x10,
            FlydigiButton::M4 => 0x20,
        }
    }
}

impl Display for FlydigiButton {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(match self {
            FlydigiButton::C => "C",
            FlydigiButton::Z => "Z",
            FlydigiButton::M1 => "M1",
            FlydigiButton::M2 => "M2",
            FlydigiButton::M3 => "M3",
            FlydigiButton::M4 => "M4",
        })
    }
}

/// State of the extra buttons as decoded from a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FlydigiButtons {
    pub c: bool,
    pub z: bool,
    pub m1: bool,
    pub m2: bool,
    pub m3: bool,
    pub m4: bool,
}

impl FlydigiButtons {
    /// Returns whether `button` is pressed.
    pub fn is_pressed(self, button: FlydigiButton) -> bool {
        match button {
            FlydigiButton::C => self.c,
            FlydigiButton::Z => self.z,
            FlydigiButton::M1 => self.m1,
            FlydigiButton::M2 => self.m2,
            FlydigiButton::M3 => self.m3,
            FlydigiButton::M4 => self.m4,
        }
    }

    /// Sets whether `button` is pressed.
    pub fn set(&mut self, button: FlydigiButton, pressed: bool) {
        match button {
            FlydigiButton::C => self.c = pressed,
            FlydigiButton::Z => self.z = pressed,
            FlydigiButton::M1 => self.m1 = pressed,
            FlydigiButton::M2 => self.m2 = pressed,
            FlydigiButton::M3 => self.m3 = pressed,
            FlydigiButton::M4 => self.m4 = pressed,
        }
    }

    /// Iterates over all buttons and their state.
    pub fn iter(self) -> impl Iterator<Item = (FlydigiButton, bool)> {
        FlydigiButton::ALL
            .into_iter()
            .map(move |button| (button, self.is_pressed(button)))
    }

    /// Decodes the extra button bits of a `V1` report into a button set.
    ///
    /// Bits 6 and 7 (`LM`/`RM`) exist only on later devices and are ignored - they
    /// are not part of [`FlydigiButton::ALL`]. Unknown bits are ignored on purpose so
    /// that a future firmware revision that starts using them cannot produce bogus
    /// button presses.
    pub const fn from_bits(bits: u8) -> Self {
        FlydigiButtons {
            c: bits & 0x01 != 0,
            z: bits & 0x02 != 0,
            m1: bits & 0x04 != 0,
            m2: bits & 0x08 != 0,
            m3: bits & 0x10 != 0,
            m4: bits & 0x20 != 0,
        }
    }

    /// Encodes the button set back into the extra button byte.
    ///
    /// Only bits of known buttons are set. Used by tests to round trip a state.
    pub const fn to_bits(self) -> u8 {
        let mut bits = 0;
        if self.c {
            bits |= 0x01;
        }
        if self.z {
            bits |= 0x02;
        }
        if self.m1 {
            bits |= 0x04;
        }
        if self.m2 {
            bits |= 0x08;
        }
        if self.m3 {
            bits |= 0x10;
        }
        if self.m4 {
            bits |= 0x20;
        }
        bits
    }
}

/// Decoded Flydigi vendor input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FlydigiInput {
    pub buttons: FlydigiButtons,
}

/// Reason why a vendor report could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeError {
    /// The report is too short to contain the fields the decoder needs.
    TooShort,
    /// The report has a different report ID / framing than expected.
    UnexpectedReport,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(match self {
            DecodeError::TooShort => "vendor report is too short",
            DecodeError::UnexpectedReport => "unexpected vendor report framing",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Decodes raw vendor HID reports into [`FlydigiInput`].
///
/// Transport code must not know anything about report layouts; it only feeds raw
/// bytes into an implementation of this trait. This keeps the protocol knowledge in
/// one place and makes it possible to support additional models/revisions without
/// touching the OS specific code.
pub trait ReportDecoder {
    /// Decodes a single report.
    fn decode(&self, report: &[u8]) -> Result<FlydigiInput, DecodeError>;
}

/// One button state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonEvent {
    Pressed(FlydigiButton),
    Released(FlydigiButton),
}

/// Edge detector for [`FlydigiButtons`].
///
/// HID reports describe the *current* state of the controller, while gilrs events
/// describe *changes*. Feeding every report through this type produces exactly one
/// `Pressed`/`Released` event per transition and nothing at all while a button is
/// held down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ButtonState {
    previous: FlydigiButtons,
}

impl ButtonState {
    /// Creates a state with nothing pressed.
    pub const fn new() -> Self {
        ButtonState {
            previous: FlydigiButtons {
                c: false,
                z: false,
                m1: false,
                m2: false,
                m3: false,
                m4: false,
            },
        }
    }

    /// Returns the last observed button state.
    pub fn previous(&self) -> FlydigiButtons {
        self.previous
    }

    /// Compares `current` with the previous state, stores it and returns the events
    /// that have to be emitted.
    ///
    /// Pressed events are returned before released events for the same report.
    pub fn update(&mut self, current: FlydigiButtons) -> ButtonEvents {
        let events = ButtonEvents {
            previous: self.previous,
            current,
            index: 0,
        };
        self.previous = current;
        events
    }
}

/// Iterator over the events produced by [`ButtonState::update`].
#[derive(Debug, Clone, Copy)]
pub struct ButtonEvents {
    previous: FlydigiButtons,
    current: FlydigiButtons,
    index: u8,
}

impl Iterator for ButtonEvents {
    type Item = ButtonEvent;

    fn next(&mut self) -> Option<ButtonEvent> {
        // First pass reports presses, second pass reports releases.
        while (self.index as usize) < 2 * FLYDIGI_BUTTON_COUNT {
            let index = self.index as usize;
            self.index += 1;
            let button = FlydigiButton::ALL[index % FLYDIGI_BUTTON_COUNT];
            let was_pressed = self.previous.is_pressed(button);
            let is_pressed = self.current.is_pressed(button);

            match (index < FLYDIGI_BUTTON_COUNT, was_pressed, is_pressed) {
                (true, false, true) => return Some(ButtonEvent::Pressed(button)),
                (false, true, false) => return Some(ButtonEvent::Released(button)),
                _ => continue,
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_round_trip() {
        for bits in 0u16..=0xff {
            let bits = bits as u8;
            let buttons = FlydigiButtons::from_bits(bits);
            assert_eq!(buttons.to_bits(), bits & 0x3f);
        }
    }

    #[test]
    fn button_masks_are_unique() {
        let mut seen = 0u8;
        for button in FlydigiButton::ALL {
            assert_eq!(seen & button.mask(), 0, "duplicated mask for {button}");
            seen |= button.mask();
        }
        assert_eq!(seen, 0x3f);
    }

    #[test]
    fn from_bits_ignores_unknown_bits() {
        // 0x40/0x80 are LM/RM on later devices and must not produce buttons.
        let buttons = FlydigiButtons::from_bits(0xc0);
        assert_eq!(buttons, FlydigiButtons::default());
        assert_eq!(buttons.to_bits(), 0);
    }

    #[test]
    fn no_events_when_nothing_changes() {
        let mut state = ButtonState::new();
        let all = FlydigiButtons {
            c: true,
            z: true,
            m1: true,
            m2: true,
            m3: true,
            m4: true,
        };

        assert_eq!(state.update(all).count(), 6);
        assert_eq!(state.update(all).count(), 0);
        assert_eq!(state.update(all).count(), 0);
        assert_eq!(state.update(FlydigiButtons::default()).count(), 6);
        assert_eq!(state.update(FlydigiButtons::default()).count(), 0);
    }

    #[test]
    fn single_press_and_release() {
        let mut state = ButtonState::new();

        let pressed = FlydigiButtons {
            m2: true,
            ..Default::default()
        };
        assert_eq!(
            state.update(pressed).collect::<Vec<_>>(),
            vec![ButtonEvent::Pressed(FlydigiButton::M2)]
        );

        assert_eq!(
            state.update(FlydigiButtons::default()).collect::<Vec<_>>(),
            vec![ButtonEvent::Released(FlydigiButton::M2)]
        );
    }

    #[test]
    fn multiple_simultaneous_changes() {
        let mut state = ButtonState::new();
        let mut current = FlydigiButtons {
            m1: true,
            m2: true,
            ..Default::default()
        };
        state.update(current);

        current.m1 = false;
        current.m2 = false;
        current.c = true;
        current.z = true;
        let events = state.update(current).collect::<Vec<_>>();

        assert!(events.contains(&ButtonEvent::Released(FlydigiButton::M1)));
        assert!(events.contains(&ButtonEvent::Released(FlydigiButton::M2)));
        assert!(events.contains(&ButtonEvent::Pressed(FlydigiButton::C)));
        assert!(events.contains(&ButtonEvent::Pressed(FlydigiButton::Z)));
        assert_eq!(events.len(), 4);
    }

    #[test]
    fn presses_come_before_releases() {
        let mut state = ButtonState::new();
        let mut current = FlydigiButtons {
            m1: true,
            ..Default::default()
        };
        state.update(current);

        current.m1 = false;
        current.c = true;
        let events = state.update(current).collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                ButtonEvent::Pressed(FlydigiButton::C),
                ButtonEvent::Released(FlydigiButton::M1),
            ]
        );
    }
}
