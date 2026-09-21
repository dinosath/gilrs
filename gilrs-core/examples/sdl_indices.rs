//! Prints the raw native button/axis list of the first connected gamepad, with the
//! index each element would have as an SDL mapping `bN`/`aN` reference.
//!
//! `gilrs`'s mapping layer (both the default mapping and `parse_sdl_mapping`) indexes
//! into exactly the `Vec`s this prints, so this is the ground truth needed to write an
//! SDL_GameControllerDB entry - guessing indices from event logs alone is not reliable
//! because the collection order depends on the platform backend, not just on the
//! usage numbers of the underlying HID elements.
//!
//! ```sh
//! cargo run --example sdl_indices
//! ```

use std::time::{Duration, Instant};

use gilrs_core::Gilrs;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

fn main() {
    env_logger::init();

    let mut gilrs = Gilrs::new().unwrap();

    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    while gilrs.gamepad(0).is_none() && Instant::now() < deadline {
        gilrs.next_event_blocking(Some(Duration::from_millis(100)));
    }

    let Some(gamepad) = gilrs.gamepad(0) else {
        eprintln!("No gamepad connected.");
        std::process::exit(1);
    };

    println!(
        "{} (vid {:04x}, pid {:04x})\n",
        gamepad.name(),
        gamepad.vendor_id().unwrap_or(0),
        gamepad.product_id().unwrap_or(0)
    );

    println!("buttons (SDL `bN`):");
    for (i, code) in gamepad.buttons().iter().enumerate() {
        println!("  b{i:<3} {code}");
    }

    println!("\naxes (SDL `aN`):");
    for (i, code) in gamepad.axes().iter().enumerate() {
        println!("  a{i:<3} {code}");
    }
}
