// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Decoder for the first generation Flydigi vendor protocol.
//!
//! # Evidence
//!
//! The layout below is implemented identically by three independent projects, which
//! is why it is treated as `CONFIRMED`:
//!
//! * SDL3 `src/joystick/hidapi/SDL_hidapi_flydigi.c`,
//!   `HIDAPI_DriverFlydigi_HandlePacketV1` - rejects anything that is not
//!   `data[0] == 0x04 && data[1] == 0xFE` and reads the extra buttons from `data[7]`
//!   (`0x04`/`0x08`/`0x10`/`0x20` = M1-M4, `0x01`/`0x02` = C/Z when the model has
//!   them).
//! * `dantmnf/Vader4ProReader` - `Vader4ProReport` documents the same 32 byte report
//!   with `buttons0` at offset 7 (`C = 1, Z = 2, M1 = 4, M2 = 8, M3 = 16, M4 = 32`).
//! * `BANANASJIM/flydigi-vader5` `docs/protocol.md` - Interface 0/EP1 standard input
//!   report for the same controller family.
//!
//! # Report layout (`CONFIRMED`)
//!
//! ```text
//! offset  size  content
//! ------  ----  -------
//! 0       1     report ID (0x04)
//! 1       1     0xFE (fixed marker, checked by SDL3)
//! 2       1     unknown (observed 0x66)
//! 3       1     air mouse active flag (0x80) on the Vader 4 Pro
//! 4..6    3     legacy motion data
//! 7       1     extra buttons: C, Z, M1, M2, M3, M4
//! 8       1     system buttons: 0x01 = Fn/'+', 0x08 = Home
//! 9       1     D-Pad (low nibble) + A/B/Select/X
//! 10      1     Y/Start/LB/RB/LT/RT/LS/RS
//! 11..31  ..    accelerometer, sticks, triggers, gyroscope
//! ```
//!
//! Only offset 0, 1 and 7 are used by this decoder. Everything else is left to the
//! platform's normal gamepad handling, so this decoder cannot interfere with the
//! standard buttons, sticks or triggers.

use super::types::{DecodeError, FlydigiButtons, FlydigiInput, ReportDecoder};

/// HID report ID of the `V1` vendor input report. CONFIRMED (SDL3, Vader4ProReader).
pub const REPORT_ID: u8 = 0x04;

/// Second byte of the `V1` vendor input report. CONFIRMED (SDL3).
pub const REPORT_MARKER: u8 = 0xfe;

/// Offset of the extra button byte. CONFIRMED (SDL3, Vader4ProReader, flydigi-vader5).
pub const EXTRA_BUTTONS_OFFSET: usize = 7;

/// Length of the report as observed for this controller family.
pub const REPORT_LEN: usize = 32;

/// The decoder needs the report ID, the marker and the extra button byte.
pub const MIN_REPORT_LEN: usize = EXTRA_BUTTONS_OFFSET + 1;

/// Returns `true` if `report` looks like a `V1` vendor input report.
///
/// This is deliberately strict (report ID *and* marker byte) because it is used to
/// decide whether a device that only matched by VID/PID really speaks this protocol.
pub fn is_v1_input_report(report: &[u8]) -> bool {
    report.len() >= MIN_REPORT_LEN && report[0] == REPORT_ID && report[1] == REPORT_MARKER
}

/// Decodes the extra buttons (`C`, `Z`, `M1`-`M4`) from a `V1` report.
///
/// Returns `None` if the report is too short or does not carry the expected report
/// ID/marker. Never panics, whatever the length of `report`.
pub fn decode_extra_buttons(report: &[u8]) -> Option<FlydigiButtons> {
    if !is_v1_input_report(report) {
        return None;
    }

    Some(FlydigiButtons::from_bits(report[EXTRA_BUTTONS_OFFSET]))
}

/// [`ReportDecoder`] implementation for the `V1` protocol.
///
/// Used by every platform backend that can read the vendor report; the decoder itself
/// has no OS dependencies and is fully covered by unit tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Vader4ProDecoder;

impl ReportDecoder for Vader4ProDecoder {
    fn decode(&self, report: &[u8]) -> Result<FlydigiInput, DecodeError> {
        if report.len() < MIN_REPORT_LEN {
            return Err(DecodeError::TooShort);
        }

        decode_extra_buttons(report)
            .map(|buttons| FlydigiInput { buttons })
            .ok_or(DecodeError::UnexpectedReport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flydigi::types::FlydigiButton;

    /// Builds a minimal synthetic `V1` report with the given extra button byte.
    fn report_with(bits: u8) -> [u8; REPORT_LEN] {
        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        report[1] = REPORT_MARKER;
        report[2] = 0x66;
        report[EXTRA_BUTTONS_OFFSET] = bits;
        report
    }

    fn buttons(bits: u8) -> FlydigiButtons {
        FlydigiButtons::from_bits(bits)
    }

    #[test]
    fn every_individual_bit() {
        let cases = [
            (0x01, FlydigiButton::C),
            (0x02, FlydigiButton::Z),
            (0x04, FlydigiButton::M1),
            (0x08, FlydigiButton::M2),
            (0x10, FlydigiButton::M3),
            (0x20, FlydigiButton::M4),
        ];

        for (bits, expected) in cases {
            let decoded = decode_extra_buttons(&report_with(bits)).unwrap();
            for button in FlydigiButton::ALL {
                assert_eq!(
                    decoded.is_pressed(button),
                    button == expected,
                    "bits {bits:#04x} decoded as {decoded:?}"
                );
            }
        }
    }

    #[test]
    fn combinations() {
        assert_eq!(
            decode_extra_buttons(&report_with(0x03)).unwrap(),
            FlydigiButtons {
                c: true,
                z: true,
                ..Default::default()
            }
        );

        assert_eq!(
            decode_extra_buttons(&report_with(0x0c)).unwrap(),
            FlydigiButtons {
                m1: true,
                m2: true,
                ..Default::default()
            }
        );

        assert_eq!(
            decode_extra_buttons(&report_with(0x30)).unwrap(),
            FlydigiButtons {
                m3: true,
                m4: true,
                ..Default::default()
            }
        );

        let mut all = FlydigiButtons {
            c: true,
            z: true,
            ..Default::default()
        };
        all.m1 = true;
        all.m2 = true;
        all.m3 = true;
        all.m4 = true;
        assert_eq!(decode_extra_buttons(&report_with(0x3f)).unwrap(), all);
    }

    #[test]
    fn all_zero_means_no_buttons() {
        assert_eq!(
            decode_extra_buttons(&report_with(0x00)).unwrap(),
            FlydigiButtons::default()
        );
    }

    #[test]
    fn all_ones_reports_extension_bits_as_unknown() {
        // 0x40/0x80 are LM/RM on the Vader 5 Pro; on a V4 Pro they are undefined.
        let decoded = decode_extra_buttons(&report_with(0xff)).unwrap();
        assert_eq!(decoded.to_bits(), 0x3f);
        assert!(FlydigiButton::ALL.iter().all(|b| decoded.is_pressed(*b)));
    }

    #[test]
    fn short_reports_are_rejected_without_panicking() {
        for len in 0..MIN_REPORT_LEN {
            let report = report_with(0x3f);
            assert_eq!(decode_extra_buttons(&report[..len]), None, "len {len}");
            assert_eq!(
                Vader4ProDecoder.decode(&report[..len]),
                Err(DecodeError::TooShort)
            );
        }
    }

    #[test]
    fn longer_reports_are_accepted() {
        let mut report = vec![0u8; 64];
        report[0] = REPORT_ID;
        report[1] = REPORT_MARKER;
        report[EXTRA_BUTTONS_OFFSET] = 0x04;
        assert_eq!(decode_extra_buttons(&report), Some(buttons(0x04)));
    }

    #[test]
    fn unexpected_report_ids_are_rejected() {
        let mut report = report_with(0x3f);
        report[0] = 0x05; // a Flydigi command report ID, not an input report
        assert_eq!(decode_extra_buttons(&report), None);
        assert_eq!(
            Vader4ProDecoder.decode(&report),
            Err(DecodeError::UnexpectedReport)
        );

        let mut report = report_with(0x3f);
        report[1] = 0x00;
        assert_eq!(decode_extra_buttons(&report), None);
    }

    #[test]
    fn every_combination_round_trips() {
        for bits in 0u16..=0x3f {
            let bits = bits as u8;
            let report = report_with(bits);
            let decoded = decode_extra_buttons(&report).unwrap();
            assert_eq!(decoded, buttons(bits));
            assert_eq!(decoded.to_bits(), bits);
        }
    }

    #[test]
    fn unknown_bits_do_not_produce_buttons() {
        for bits in 0x40u8..=0xff {
            let decoded = decode_extra_buttons(&report_with(bits)).unwrap();
            let known = FlydigiButtons::from_bits(bits);
            assert_eq!(decoded, known);
            assert_eq!(decoded.to_bits(), bits & 0x3f);
        }
    }

    #[test]
    fn decoder_trait_matches_free_function() {
        for bits in 0u16..=0xff {
            let bits = bits as u8;
            let report = report_with(bits);
            assert_eq!(
                Vader4ProDecoder.decode(&report).unwrap(),
                FlydigiInput {
                    buttons: decode_extra_buttons(&report).unwrap()
                }
            );
        }
    }

    #[test]
    fn simultaneous_extra_and_standard_buttons_do_not_interfere() {
        // The standard buttons live in other bytes; changing them must not change the
        // decoded extra buttons.
        let mut report = report_with(0x08);
        report[9] = 0x10 | 0x20; // A + B
        report[10] = 0x04 | 0x08; // LB + RB
        report[15] = 0xff; // left trigger
        report[16] = 0xff; // right trigger

        let decoded = decode_extra_buttons(&report).unwrap();
        assert!(decoded.m2);
        assert_eq!(decoded.to_bits(), 0x08);
    }
}
