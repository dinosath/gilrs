//! Interactive connectivity and input test.
//!
//! Prints every connected gamepad, every connect/disconnect and every button or
//! axis change, plus a periodic snapshot of the current state:
//!
//! ```sh
//! cargo run --example gamepad_test
//! ```
//!
//! Add `RUST_LOG=gilrs=debug,gilrs_core=debug` for backend logs (useful when a
//! controller is not detected at all). Press Ctrl-C to quit.

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, Event, EventType, Gilrs, PowerInfo};

const BUTTONS: [Button; 23] = [
    Button::South,
    Button::East,
    Button::North,
    Button::West,
    Button::C,
    Button::Z,
    Button::LeftTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::Mode,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::M1,
    Button::M2,
    Button::M3,
    Button::M4,
];

const AXES: [Axis; 8] = [
    Axis::LeftStickX,
    Axis::LeftStickY,
    Axis::LeftZ,
    Axis::RightStickX,
    Axis::RightStickY,
    Axis::RightZ,
    Axis::DPadX,
    Axis::DPadY,
];

const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(2);

fn main() {
    env_logger::init();

    let mut gilrs = match Gilrs::new() {
        Ok(gilrs) => gilrs,
        Err(gilrs::Error::NotImplemented(gilrs)) => {
            eprintln!("warning: this platform is not supported, no events will be reported");
            gilrs
        }
        Err(e) => {
            eprintln!("failed to create gilrs context: {e}");
            std::process::exit(1);
        }
    };

    let connected = gilrs.gamepads().count();
    if connected == 0 {
        println!("No gamepad connected yet - plug one in or turn it on.");
    } else {
        println!("{connected} gamepad(s) connected:");
        for (id, gamepad) in gilrs.gamepads() {
            print_info(id, &gamepad);
        }
    }

    println!("\nPress buttons and move sticks (Ctrl-C to quit).\n");

    let mut last_snapshot = Instant::now();

    loop {
        while let Some(Event { id, event, .. }) = gilrs.next_event_blocking(Some(SNAPSHOT_INTERVAL))
        {
            match event {
                EventType::Connected => {
                    println!("[{id}] connected");
                    print_info(id, &gilrs.gamepad(id));
                }
                EventType::Disconnected => println!("[{id}] disconnected"),
                EventType::ButtonPressed(button, code) => {
                    println!("[{id}] {button:?} pressed  ({code})")
                }
                EventType::ButtonReleased(button, code) => {
                    println!("[{id}] {button:?} released ({code})")
                }
                EventType::ButtonChanged(button, value, code) => {
                    println!("[{id}] {button:?} = {value:.3} ({code})")
                }
                EventType::AxisChanged(axis, value, code) => {
                    println!("[{id}] {axis:?} = {value:+.3} ({code})")
                }
                other => println!("[{id}] {other:?}"),
            }

            if last_snapshot.elapsed() >= SNAPSHOT_INTERVAL {
                break;
            }
        }

        last_snapshot = Instant::now();
        for (id, gamepad) in gilrs.gamepads() {
            print_state(id, &gamepad);
        }
    }
}

fn print_info(id: gilrs::GamepadId, gamepad: &gilrs::Gamepad<'_>) {
    println!("  [{id}] {}", gamepad.name());
    println!("      os name:   {}", gamepad.os_name());
    println!(
        "      vid:pid:   {}",
        match (gamepad.vendor_id(), gamepad.product_id()) {
            (Some(vid), Some(pid)) => format!("{vid:04x}:{pid:04x}"),
            _ => "unknown".to_string(),
        }
    );
    println!(
        "      uuid:      {}",
        gamepad
            .uuid()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    println!("      mapping:   {:?}", gamepad.mapping_source());
    println!(
        "      map name:  {}",
        gamepad.map_name().unwrap_or("<none>")
    );
    println!("      force fb:  {}", gamepad.is_ff_supported());
    println!("      power:     {}", power(gamepad.power_info()));
}

fn print_state(id: gilrs::GamepadId, gamepad: &gilrs::Gamepad<'_>) {
    let pressed: Vec<_> = BUTTONS
        .iter()
        .filter(|b| gamepad.is_pressed(**b))
        .map(|b| format!("{b:?}"))
        .collect();

    let axes: Vec<_> = AXES
        .iter()
        .map(|a| (a, gamepad.value(*a)))
        .filter(|(_, v)| v.abs() > 0.01)
        .map(|(a, v)| format!("{a:?}={v:+.2}"))
        .collect();

    println!(
        "--- [{id}] {} | pressed: {} | axes: {}",
        gamepad.name(),
        if pressed.is_empty() {
            "-".to_string()
        } else {
            pressed.join(", ")
        },
        if axes.is_empty() {
            "-".to_string()
        } else {
            axes.join(", ")
        }
    );
}

fn power(info: PowerInfo) -> String {
    match info {
        PowerInfo::Unknown => "unknown".to_string(),
        PowerInfo::Wired => "wired".to_string(),
        PowerInfo::Discharging(p) => format!("discharging ({p}%)"),
        PowerInfo::Charging(p) => format!("charging ({p}%)"),
        PowerInfo::Charged => "charged".to_string(),
    }
}
