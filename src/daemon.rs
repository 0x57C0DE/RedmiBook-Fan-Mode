//! The system service: waits for Fn+K, reads the new mode and shows it.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::ec::{Ec, MODE_OFFSET};
use crate::hardware;
use crate::keyboard::Keyboard;
use crate::mode::Mode;
use crate::notify::Notifier;

/// One Fn+K press produces several scancodes; ignore repeats within this window.
const DEBOUNCE: Duration = Duration::from_millis(500);

/// How long to wait for the EC to report a new mode after the scancode.
const SETTLE: Duration = Duration::from_secs(2);

pub fn run() -> Result<()> {
    hardware::warn_if_untested();
    let ec = Ec::open()?;
    let mut keyboard = Keyboard::open()?;
    let mut notifier = Notifier::new()?;
    let mut mode = Mode::from_raw(ec.read_byte(MODE_OFFSET)?);
    println!("started, current mode: {mode}");

    let mut last_press: Option<Instant> = None;
    loop {
        if !keyboard.wait_fn_k(None)? {
            continue;
        }
        if last_press.is_some_and(|t| t.elapsed() < DEBOUNCE) {
            continue;
        }
        mode = wait_for_change(&ec, mode)?;
        last_press = Some(Instant::now());
        println!("Fn+K, mode: {mode}");
        notifier.show(mode);
    }
}

/// The EC often flips the byte before the scancode arrives, so compare with
/// the last mode shown instead of sleeping a fixed time.
fn wait_for_change(ec: &Ec, previous: Mode) -> Result<Mode> {
    let deadline = Instant::now() + SETTLE;
    loop {
        let mode = Mode::from_raw(ec.read_byte(MODE_OFFSET)?);
        if mode != previous || Instant::now() >= deadline {
            return Ok(mode);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
