// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! GameController.framework backend, for iOS.
//!
//! The macOS backend reads IOKit HID, which iOS does not expose. What it does
//! expose is GameController.framework, and the shape of that API is much closer
//! to the web's: it reports the current state of a pad rather than a stream of
//! events. So this backend is modelled on the wasm one — each call to
//! `next_event` samples the connected pads and turns the difference from the
//! last sample into events.
//!
//! The framework hands over buttons already named — `buttonA`, `leftShoulder`,
//! `dpad.up` — so no SDL mapping is involved and the codes below are reported
//! straight. Pads therefore carry a nil UUID, which is what puts gilrs on its
//! default mapping rather than a database lookup.
//!
//! `Gilrs` has to be `Send`, and an Objective-C object is neither `Send` nor
//! `Sync`, so nothing here holds one: a `Gamepad` keeps the address of the
//! framework's instance and looks the object back up when it needs it, the way
//! the macOS backend keeps plain data and leaves the IOKit handles to its own
//! thread.

use std::collections::VecDeque;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::thread::sleep;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2_game_controller::{GCController, GCDevice, GCDeviceBatteryState, GCExtendedGamepad};
use uuid::Uuid;

use super::FfDevice;
use crate::{AxisInfo, Event, EventType, PlatformError, PowerInfo};
#[cfg(feature = "serde-serialize")]
use serde::{Deserialize, Serialize};

/// How long `next_event_blocking` waits between samples. The framework has no
/// blocking read, so the wait is a poll; a frame at 60 Hz is the interval a
/// caller would have polled at anyway.
const POLL_INTERVAL: Duration = Duration::from_millis(16);

/// The framework's instance for `address`, if that pad is still connected.
///
/// The address is only ever compared, never dereferenced: a stale one simply
/// matches nothing.
fn controller_at(address: usize) -> Option<Retained<GCController>> {
    // SAFETY: a plain read of the framework's list of connected devices.
    unsafe { GCController::controllers() }
        .iter()
        .find(|controller| Retained::as_ptr(controller) as usize == address)
}

#[derive(Debug)]
pub struct Gilrs {
    event_cache: VecDeque<Event>,
    gamepads: Vec<Gamepad>,
}

impl Gilrs {
    pub(crate) fn new() -> Result<Self, PlatformError> {
        Ok(Gilrs {
            event_cache: VecDeque::new(),
            gamepads: Vec::new(),
        })
    }

    pub(crate) fn next_event(&mut self) -> Option<Event> {
        // Sampling every pad is the expensive part, so it only happens once the
        // events the last sample produced have all been handed out.
        if let Some(event) = self.event_cache.pop_front() {
            return Some(event);
        }

        self.sample();
        self.event_cache.pop_front()
    }

    pub(crate) fn next_event_blocking(&mut self, timeout: Option<Duration>) -> Option<Event> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        loop {
            if let Some(event) = self.next_event() {
                return Some(event);
            }
            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return None;
                }
                sleep(POLL_INTERVAL.min(deadline - now));
            } else {
                sleep(POLL_INTERVAL);
            }
        }
    }

    pub fn gamepad(&self, id: usize) -> Option<&Gamepad> {
        self.gamepads.get(id)
    }

    /// Returns index greater than index of last connected gamepad.
    pub fn last_gamepad_hint(&self) -> usize {
        self.gamepads.len()
    }

    /// Reads every connected pad and queues what changed since the last read.
    ///
    /// A gamepad is never removed from `gamepads`: an id gilrs has handed out
    /// has to keep meaning the same pad, so a disconnected one stays in place
    /// and is reused if it comes back.
    fn sample(&mut self) {
        // SAFETY: a plain read of the framework's list of connected devices.
        let controllers = unsafe { GCController::controllers() };

        let mut seen = vec![false; self.gamepads.len()];
        for controller in controllers.iter() {
            // The framework keeps one instance per connected pad, so its
            // address identifies it for as long as it is plugged in.
            let address = Retained::as_ptr(&controller) as usize;
            match self
                .gamepads
                .iter()
                .position(|gamepad| gamepad.address == address)
            {
                Some(id) => {
                    seen[id] = true;
                    self.update(id, &controller);
                }
                None => {
                    let id = self.adopt(&controller, address);
                    debug!("gamepad {id} connected: {}", self.gamepads[id].name());
                    self.event_cache
                        .push_back(Event::new(id, EventType::Connected));
                }
            }
        }

        for (id, gamepad) in self.gamepads.iter_mut().enumerate() {
            if seen.get(id).copied().unwrap_or(false) || !gamepad.connected {
                continue;
            }
            debug!("gamepad {id} disconnected: {}", gamepad.name());
            gamepad.connected = false;
            gamepad.state = State::default();
            self.event_cache
                .push_back(Event::new(id, EventType::Disconnected));
        }
    }

    /// Takes an id for a pad that was not there a moment ago, reusing the slot
    /// of one that has since disconnected rather than growing forever.
    fn adopt(&mut self, controller: &GCController, address: usize) -> usize {
        let gamepad = Gamepad::new(controller, address);
        match self
            .gamepads
            .iter()
            .position(|existing| !existing.connected && existing.name == gamepad.name)
        {
            Some(id) => {
                self.gamepads[id] = gamepad;
                id
            }
            None => {
                self.gamepads.push(gamepad);
                self.gamepads.len() - 1
            }
        }
    }

    fn update(&mut self, id: usize, controller: &GCController) {
        let gamepad = &mut self.gamepads[id];
        if !gamepad.connected {
            gamepad.connected = true;
            self.event_cache
                .push_back(Event::new(id, EventType::Connected));
        }

        // SAFETY: a plain read of the controller's profile.
        let Some(profile) = (unsafe { controller.extendedGamepad() }) else {
            // A pad with no extended profile — a Siri Remote, say — reports
            // nothing rather than being described by codes that do not fit it.
            return;
        };
        // SAFETY: the profile is alive for the length of this call.
        let state = unsafe { State::read(&profile) };
        let was = gamepad.state;
        gamepad.state = state;

        for (index, code) in native_ev_codes::BUTTONS.iter().enumerate() {
            match (was.buttons[index], state.buttons[index]) {
                (false, true) => self.event_cache.push_back(Event::new(
                    id,
                    EventType::ButtonPressed(crate::EvCode(*code)),
                )),
                (true, false) => self.event_cache.push_back(Event::new(
                    id,
                    EventType::ButtonReleased(crate::EvCode(*code)),
                )),
                _ => (),
            }
        }

        // The triggers are analog, so they are reported as axes as well and
        // gilrs derives the button events from those.
        for (value, was, code) in [
            (
                state.left_trigger,
                was.left_trigger,
                native_ev_codes::BTN_LT2,
            ),
            (
                state.right_trigger,
                was.right_trigger,
                native_ev_codes::BTN_RT2,
            ),
        ] {
            if value != was {
                self.event_cache.push_back(Event::new(
                    id,
                    EventType::AxisValueChanged(trigger_value(value), crate::EvCode(code)),
                ));
            }
        }

        for (index, code) in native_ev_codes::AXES.iter().enumerate() {
            if state.axes[index] != was.axes[index] {
                self.event_cache.push_back(Event::new(
                    id,
                    EventType::AxisValueChanged(
                        axis_value(state.axes[index]),
                        crate::EvCode(*code),
                    ),
                ));
            }
        }
    }
}

/// Scales a stick, which rests at 0 and runs to ±1, onto the full range.
fn axis_value(value: f32) -> i32 {
    (f64::from(value.clamp(-1.0, 1.0)) * f64::from(i32::MAX)) as i32
}

/// Scales a trigger, which rests at 0 and runs to 1, onto the positive range.
fn trigger_value(value: f32) -> i32 {
    (f64::from(value.clamp(0.0, 1.0)) * f64::from(i32::MAX)) as i32
}

/// One sample of a pad, kept so the next one can be reported as a difference.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
struct State {
    buttons: [bool; native_ev_codes::BUTTONS.len()],
    axes: [f32; native_ev_codes::AXES.len()],
    left_trigger: f32,
    right_trigger: f32,
}

impl State {
    /// Reads the pad, in the order of [`native_ev_codes::BUTTONS`] and
    /// [`native_ev_codes::AXES`].
    ///
    /// # Safety
    ///
    /// The caller holds a live profile.
    unsafe fn read(profile: &GCExtendedGamepad) -> Self {
        // SAFETY: the caller's guarantee; every call below reads a current
        // value from that live profile.
        unsafe {
            let dpad = profile.dpad();
            let left_stick = profile.leftThumbstick();
            let right_stick = profile.rightThumbstick();
            let optional = |button: Option<Retained<_>>| {
                button.is_some_and(
                    |button: Retained<objc2_game_controller::GCControllerButtonInput>| {
                        button.isPressed()
                    },
                )
            };

            State {
                buttons: [
                    profile.buttonA().isPressed(),
                    profile.buttonB().isPressed(),
                    profile.buttonX().isPressed(),
                    profile.buttonY().isPressed(),
                    profile.leftShoulder().isPressed(),
                    profile.rightShoulder().isPressed(),
                    profile.leftTrigger().isPressed(),
                    profile.rightTrigger().isPressed(),
                    optional(profile.buttonOptions()),
                    profile.buttonMenu().isPressed(),
                    optional(profile.leftThumbstickButton()),
                    optional(profile.rightThumbstickButton()),
                    dpad.up().isPressed(),
                    dpad.down().isPressed(),
                    dpad.left().isPressed(),
                    dpad.right().isPressed(),
                    optional(profile.buttonHome()),
                ],
                axes: [
                    left_stick.xAxis().value(),
                    left_stick.yAxis().value(),
                    right_stick.xAxis().value(),
                    right_stick.yAxis().value(),
                ],
                left_trigger: profile.leftTrigger().value(),
                right_trigger: profile.rightTrigger().value(),
            }
        }
    }
}

#[derive(Debug)]
pub struct Gamepad {
    /// The framework's instance address, which identifies the pad while it is
    /// connected. Only compared, never dereferenced — holding the object itself
    /// would cost `Gilrs` its `Send`.
    address: usize,
    name: String,
    connected: bool,
    state: State,
}

impl Gamepad {
    fn new(controller: &GCController, address: usize) -> Gamepad {
        // SAFETY: a plain read of the device's own description.
        let name = unsafe { controller.vendorName() }
            .map(|name| name.to_string())
            .unwrap_or_else(|| String::from("Game Controller"));

        Gamepad {
            address,
            name,
            connected: true,
            state: State::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// GameController describes a pad by capability rather than by a USB
    /// identity, so there is nothing to build a stable UUID out of. Nil is what
    /// puts gilrs on its default mapping, which is what suits an API whose
    /// buttons are already named.
    pub fn uuid(&self) -> Uuid {
        Uuid::nil()
    }

    pub fn vendor_id(&self) -> Option<u16> {
        None
    }

    pub fn product_id(&self) -> Option<u16> {
        None
    }

    pub fn power_info(&self) -> PowerInfo {
        let Some(controller) = controller_at(self.address) else {
            return PowerInfo::Unknown;
        };
        // SAFETY: plain reads of the device's battery, which not every pad has.
        let Some(battery) = (unsafe { controller.battery() }) else {
            return PowerInfo::Unknown;
        };
        // SAFETY: as above.
        let (level, state) = unsafe { (battery.batteryLevel(), battery.batteryState()) };
        let percent = (level.clamp(0.0, 1.0) * 100.0).round() as u8;
        match state {
            GCDeviceBatteryState::Discharging => PowerInfo::Discharging(percent),
            GCDeviceBatteryState::Charging => PowerInfo::Charging(percent),
            GCDeviceBatteryState::Full => PowerInfo::Charged,
            _ => PowerInfo::Unknown,
        }
    }

    pub fn is_ff_supported(&self) -> bool {
        false
    }

    /// Creates Ffdevice corresponding to this gamepad.
    pub fn ff_device(&self) -> Option<FfDevice> {
        None
    }

    pub fn buttons(&self) -> &[EvCode] {
        &native_ev_codes::BUTTONS
    }

    pub fn axes(&self) -> &[EvCode] {
        &native_ev_codes::AXES
    }

    pub(crate) fn axis_info(&self, nec: EvCode) -> Option<&AxisInfo> {
        // A trigger only ever rises from rest, where a stick swings both ways.
        const TRIGGER: AxisInfo = AxisInfo {
            min: 0,
            max: i32::MAX,
            deadzone: None,
        };
        const STICK: AxisInfo = AxisInfo {
            min: i32::MIN,
            max: i32::MAX,
            deadzone: None,
        };

        if nec == native_ev_codes::BTN_LT2 || nec == native_ev_codes::BTN_RT2 {
            return Some(&TRIGGER);
        }
        native_ev_codes::AXES.contains(&nec).then_some(&STICK)
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EvCode(u8);

impl EvCode {
    pub fn into_u32(self) -> u32 {
        self.0 as u32
    }
}

impl Display for EvCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        self.0.fmt(f)
    }
}

pub mod native_ev_codes {
    use super::EvCode;

    pub const AXIS_LSTICKX: EvCode = EvCode(0);
    pub const AXIS_LSTICKY: EvCode = EvCode(1);
    pub const AXIS_LEFTZ: EvCode = EvCode(2);
    pub const AXIS_RSTICKX: EvCode = EvCode(3);
    pub const AXIS_RSTICKY: EvCode = EvCode(4);
    pub const AXIS_RIGHTZ: EvCode = EvCode(5);
    pub const AXIS_DPADX: EvCode = EvCode(6);
    pub const AXIS_DPADY: EvCode = EvCode(7);
    pub const AXIS_RT: EvCode = EvCode(8);
    pub const AXIS_LT: EvCode = EvCode(9);
    pub const AXIS_RT2: EvCode = EvCode(10);
    pub const AXIS_LT2: EvCode = EvCode(11);

    pub const BTN_SOUTH: EvCode = EvCode(12);
    pub const BTN_EAST: EvCode = EvCode(13);
    pub const BTN_C: EvCode = EvCode(14);
    pub const BTN_NORTH: EvCode = EvCode(15);
    pub const BTN_WEST: EvCode = EvCode(16);
    pub const BTN_Z: EvCode = EvCode(17);
    pub const BTN_LT: EvCode = EvCode(18);
    pub const BTN_RT: EvCode = EvCode(19);
    pub const BTN_LT2: EvCode = EvCode(20);
    pub const BTN_RT2: EvCode = EvCode(21);
    pub const BTN_SELECT: EvCode = EvCode(22);
    pub const BTN_START: EvCode = EvCode(23);
    pub const BTN_MODE: EvCode = EvCode(24);
    pub const BTN_LTHUMB: EvCode = EvCode(25);
    pub const BTN_RTHUMB: EvCode = EvCode(26);

    pub const BTN_DPAD_UP: EvCode = EvCode(27);
    pub const BTN_DPAD_DOWN: EvCode = EvCode(28);
    pub const BTN_DPAD_LEFT: EvCode = EvCode(29);
    pub const BTN_DPAD_RIGHT: EvCode = EvCode(30);

    /// The order the state is read in, so a sample lines up with the last one
    /// position by position. GameController names the face buttons by their
    /// printed letter, and an Xbox pad's A sits where gilrs calls `BTN_SOUTH`.
    pub(super) static BUTTONS: [EvCode; 17] = [
        BTN_SOUTH,
        BTN_EAST,
        BTN_WEST,
        BTN_NORTH,
        BTN_LT,
        BTN_RT,
        BTN_LT2,
        BTN_RT2,
        BTN_SELECT,
        BTN_START,
        BTN_LTHUMB,
        BTN_RTHUMB,
        BTN_DPAD_UP,
        BTN_DPAD_DOWN,
        BTN_DPAD_LEFT,
        BTN_DPAD_RIGHT,
        BTN_MODE,
    ];

    pub(super) static AXES: [EvCode; 4] = [AXIS_LSTICKX, AXIS_LSTICKY, AXIS_RSTICKX, AXIS_RSTICKY];
}
