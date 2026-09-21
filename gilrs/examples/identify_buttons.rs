//! Walks through every button of a gamepad and reports what gilrs makes of it.
//!
//! For each physical button it records the native `Code` the backend produced and
//! the `Button` the mapping resolved it to, then prints a table marking anything
//! that is unmapped or mapped to the wrong button:
//!
//! ```sh
//! cargo run --example identify_buttons
//! ```
//!
//! Add `RUST_LOG=gilrs_core=debug` to also see how the controller is attached
//! (USB/Bluetooth) and which HID elements the backend ignored.
//!
//! Buttons the controller does not have can be skipped by waiting.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use gilrs::ev::Code;
use gilrs::{Axis, Button, Event, EventType, Gilrs};

/// Physical button and the `Button` it is expected to be reported as.
const CHECKS: [(&str, Button); 23] = [
    ("A / South", Button::South),
    ("B / East", Button::East),
    ("X / West", Button::West),
    ("Y / North", Button::North),
    ("LB", Button::LeftTrigger),
    ("RB", Button::RightTrigger),
    ("LT", Button::LeftTrigger2),
    ("RT", Button::RightTrigger2),
    ("Back / Select", Button::Select),
    ("Start", Button::Start),
    ("Home / Mode", Button::Mode),
    ("Left stick click", Button::LeftThumb),
    ("Right stick click", Button::RightThumb),
    ("D-pad up", Button::DPadUp),
    ("D-pad down", Button::DPadDown),
    ("D-pad left", Button::DPadLeft),
    ("D-pad right", Button::DPadRight),
    ("C", Button::C),
    ("Z", Button::Z),
    ("M1", Button::M1),
    ("M2", Button::M2),
    ("M3", Button::M3),
    ("M4", Button::M4),
];

/// Time without new events that marks the end of one button press.
const SETTLE: Duration = Duration::from_millis(400);

/// How long to wait for a press before treating the button as missing.
const SKIP_AFTER: Duration = Duration::from_secs(6);

/// How long to wait for the backend to report an already connected gamepad.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

fn main() {
    env_logger::init();

    let mut gilrs = Gilrs::new().expect("failed to create gilrs context");

    // Backends discover devices asynchronously, so the list is empty until the
    // first `Connected` event has been pumped.
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    while gilrs.gamepads().next().is_none() && Instant::now() < deadline {
        gilrs.next_event_blocking(Some(Duration::from_millis(100)));
    }

    match gilrs.gamepads().next() {
        Some((id, gamepad)) => {
            println!("[{id}] {}", gamepad.name());
            println!("  os name:  {}", gamepad.os_name());
            println!(
                "  vid:pid:  {:04x}:{:04x}",
                gamepad.vendor_id().unwrap_or(0),
                gamepad.product_id().unwrap_or(0)
            );
            println!(
                "  uuid:     {}",
                gamepad
                    .uuid()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            println!("  mapping:  {:?}", gamepad.mapping_source());
            println!("  map name: {}\n", gamepad.map_name().unwrap_or("<none>"));
        }
        None => {
            eprintln!("No gamepad connected.");
            std::process::exit(1);
        }
    }

    println!("Press each button once when asked, or wait to skip it.\n");

    let mut results = Vec::new();

    for (label, expected) in CHECKS {
        print!("Press {label:<18} ... ");
        io::stdout().flush().unwrap();

        match wait_for_press(&mut gilrs) {
            Some((reported, code)) => {
                println!("{reported:?} ({code})");
                results.push((label, expected, Some((reported, code))));
            }
            None => {
                println!("skipped");
                results.push((label, expected, None));
            }
        }
    }

    println!(
        "\n{:<18} {:<14} {:<18} {:<14} {}",
        "physical", "expected", "code", "reported", "status"
    );

    let mut problems = 0;

    for (label, expected, outcome) in &results {
        let (status, code, reported) = match outcome {
            Some((reported, code)) if reported == expected => {
                ("ok", code.to_string(), format!("{reported:?}"))
            }
            Some((Button::Unknown, code)) => {
                problems += 1;
                ("UNMAPPED", code.to_string(), "Unknown".to_string())
            }
            Some((reported, code)) => {
                problems += 1;
                ("WRONG", code.to_string(), format!("{reported:?}"))
            }
            None => ("skipped", "-".to_string(), "-".to_string()),
        };

        println!(
            "{label:<18} {:<14} {code:<18} {reported:<14} {status}",
            format!("{expected:?}")
        );
    }

    if problems == 0 {
        println!("\nAll tested buttons are mapped correctly.");
    } else {
        println!("\n{problems} button(s) are unmapped or mapped to the wrong button.");
    }
}

/// Waits for the next button press and swallows everything that follows it until
/// the device goes quiet again, so the release does not get read as a new press.
fn wait_for_press(gilrs: &mut Gilrs) -> Option<(Button, Code)> {
    let give_up = Instant::now() + SKIP_AFTER;

    let pressed = loop {
        let now = Instant::now();
        if now >= give_up {
            return None;
        }

        if let Some(Event { event, .. }) = gilrs.next_event_blocking(Some(give_up - now)) {
            match event {
                EventType::ButtonPressed(button, code) => break (button, code),
                // A D-pad is an axis pair on some platforms, and a mapping may bind a
                // trigger to an axis rather than to a button.
                EventType::AxisChanged(axis, value, code) if value.abs() > 0.5 => {
                    break (axis_as_button(axis, value), code)
                }
                _ => (),
            }
        }
    };

    let mut last = Instant::now();
    while last.elapsed() < SETTLE {
        if gilrs.next_event_blocking(Some(SETTLE)).is_some() {
            last = Instant::now();
        }
    }

    Some(pressed)
}

fn axis_as_button(axis: Axis, value: f32) -> Button {
    match (axis, value > 0.0) {
        (Axis::LeftZ, _) => Button::LeftTrigger2,
        (Axis::RightZ, _) => Button::RightTrigger2,
        (Axis::DPadX, true) => Button::DPadRight,
        (Axis::DPadX, false) => Button::DPadLeft,
        (Axis::DPadY, true) => Button::DPadUp,
        (Axis::DPadY, false) => Button::DPadDown,
        _ => Button::Unknown,
    }
}
