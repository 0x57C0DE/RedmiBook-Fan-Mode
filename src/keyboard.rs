//! Detects Fn+K on the built-in keyboard.
//!
//! The EC handles Fn+K itself and only forwards an unmapped scancode, so the
//! key never becomes a key event. It shows up as `EV_MSC`/`MSC_SCAN` with
//! value 0x86 (four of them per press, followed by 0xF6).

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};

const KEYBOARD_NAME: &str = "AT Translated Set 2 keyboard";
const FN_K_SCANCODE: i32 = 0x86;
const EV_MSC: u16 = 0x04;
const MSC_SCAN: u16 = 0x04;
const EVENT_SIZE: usize = size_of::<libc::input_event>();
// struct input_event ends with: __u16 type; __u16 code; __s32 value;
const TYPE_AT: usize = EVENT_SIZE - 8;
const CODE_AT: usize = EVENT_SIZE - 6;
const VALUE_AT: usize = EVENT_SIZE - 4;

pub struct Keyboard {
    file: File,
    buf: Vec<u8>,
}

impl Keyboard {
    /// Opens the event node of the built-in AT keyboard. Needs root or the `input` group.
    pub fn open() -> Result<Self> {
        let path = find_event_node()?;
        let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        Ok(Self {
            file,
            buf: vec![0; EVENT_SIZE * 64],
        })
    }

    /// Waits for input and reports whether it contained Fn+K. `None` waits forever.
    /// Returns `false` on timeout or for any other key.
    pub fn wait_fn_k(&mut self, timeout: Option<Duration>) -> Result<bool> {
        let timeout_ms = timeout.map_or(-1, |t| t.as_millis().min(i32::MAX as u128) as i32);
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: pfd is a valid pollfd that outlives the call, and nfds is 1.
        let ready = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ready < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(err).context("polling the keyboard");
        }
        if ready == 0 {
            return Ok(false);
        }
        let n = match self.file.read(&mut self.buf) {
            Ok(n) => n,
            Err(err) if err.kind() == ErrorKind::Interrupted => return Ok(false),
            Err(err) => return Err(err).context("reading keyboard events"),
        };
        Ok(contains_fn_k(&self.buf[..n]))
    }
}

fn contains_fn_k(data: &[u8]) -> bool {
    let (events, _) = data.as_chunks::<EVENT_SIZE>();
    events.iter().any(|ev| {
        let ev_type = u16::from_ne_bytes([ev[TYPE_AT], ev[TYPE_AT + 1]]);
        let code = u16::from_ne_bytes([ev[CODE_AT], ev[CODE_AT + 1]]);
        let value = i32::from_ne_bytes(ev[VALUE_AT..VALUE_AT + 4].try_into().unwrap());
        ev_type == EV_MSC && code == MSC_SCAN && value == FN_K_SCANCODE
    })
}

fn find_event_node() -> Result<PathBuf> {
    for entry in fs::read_dir("/sys/class/input").context("listing /sys/class/input")? {
        let dir = entry?.path();
        let name = fs::read_to_string(dir.join("name")).unwrap_or_default();
        if name.trim() != KEYBOARD_NAME {
            continue;
        }
        for child in fs::read_dir(&dir)? {
            let child = child?.file_name();
            if child.to_string_lossy().starts_with("event") {
                return Ok(PathBuf::from("/dev/input").join(child));
            }
        }
    }
    bail!("input device {KEYBOARD_NAME:?} not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(ev_type: u16, code: u16, value: i32) -> Vec<u8> {
        let mut ev = vec![0u8; EVENT_SIZE];
        ev[TYPE_AT..TYPE_AT + 2].copy_from_slice(&ev_type.to_ne_bytes());
        ev[CODE_AT..CODE_AT + 2].copy_from_slice(&code.to_ne_bytes());
        ev[VALUE_AT..VALUE_AT + 4].copy_from_slice(&value.to_ne_bytes());
        ev
    }

    #[test]
    fn detects_fn_k_scancode() {
        let mut data = event(0, 0, 0);
        data.extend(event(EV_MSC, MSC_SCAN, 0x86));
        assert!(contains_fn_k(&data));
    }

    #[test]
    fn ignores_other_scancodes_and_key_events() {
        let mut data = event(EV_MSC, MSC_SCAN, 0x25);
        data.extend(event(0x01, 0x86, 1));
        data.extend(event(EV_MSC, MSC_SCAN, 0xF6));
        assert!(!contains_fn_k(&data));
    }
}
