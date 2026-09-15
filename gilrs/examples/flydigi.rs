//! Prints Flydigi extra button events (`C`, `Z`, `M1`-`M4`).
//!
//! Useful to validate that a controller is detected and that the vendor report is
//! decoded correctly:
//!
//! ```sh
//! cargo run --example flydigi
//! ```
//!
//! Run it with `RUST_LOG=gilrs_core=info` to also see when the vendor reader is
//! attached. On Linux, `/dev/hidraw*` has to be readable; see `docs/flydigi.md`.

use gilrs::{Button, EventType, Gilrs};

fn main() {
    let mut gilrs = Gilrs::new().unwrap();

    for (id, gamepad) in gilrs.gamepads() {
        println!(
            "{id}: {} (vid {:04x}, pid {:04x}), pressed macro buttons: {:?}",
            gamepad.name(),
            gamepad.vendor_id().unwrap_or(0),
            gamepad.product_id().unwrap_or(0),
            [Button::M1, Button::M2, Button::M3, Button::M4]
                .into_iter()
                .filter(|button| gamepad.is_pressed(*button))
                .collect::<Vec<_>>()
        );
    }

    println!("Press C, Z or M1-M4 (Ctrl-C to quit):");

    loop {
        while let Some(ev) = gilrs.next_event() {
            match ev.event {
                EventType::ButtonPressed(button, _) if is_extra(button) => {
                    println!("{}: {:?} pressed", ev.id, button);
                }
                EventType::ButtonReleased(button, _) if is_extra(button) => {
                    println!("{}: {:?} released", ev.id, button);
                }
                EventType::Connected => println!("{}: connected", ev.id),
                EventType::Disconnected => println!("{}: disconnected", ev.id),
                _ => (),
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn is_extra(button: Button) -> bool {
    button.is_macro() || matches!(button, Button::C | Button::Z)
}
