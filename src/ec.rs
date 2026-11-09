//! Read-only access to the embedded controller RAM through `ec_sys` debugfs.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const EC_PATH: &str = "/sys/kernel/debug/ec/ec0/io";

/// Size of the EC RAM window exposed by `ec_sys`.
pub const EC_SIZE: usize = 256;

/// EC RAM offset of the fan and power mode that Fn+K cycles.
pub const MODE_OFFSET: u8 = 0xE4;

/// EC RAM offsets that track the speed of the two fans (unit unknown, about x100 rpm).
pub const FAN_OFFSETS: [u8; 2] = [0x9B, 0x9D];

/// Handle to the EC RAM. Every read goes straight to the EC: positional reads
/// on an unbuffered file, so a value is never served from a stale cache.
pub struct Ec {
    file: File,
}

impl Ec {
    /// Loads `ec_sys` if needed and opens the EC RAM file. Needs root.
    pub fn open() -> Result<Self> {
        if !Path::new(EC_PATH).exists() {
            let status = Command::new("modprobe")
                .arg("ec_sys")
                .status()
                .context("running modprobe ec_sys")?;
            if !status.success() {
                bail!("modprobe ec_sys failed: {status}");
            }
        }
        let file = File::open(EC_PATH).with_context(|| format!("opening {EC_PATH}"))?;
        Ok(Self { file })
    }

    pub fn read_byte(&self, offset: u8) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.file
            .read_exact_at(&mut buf, offset.into())
            .with_context(|| format!("reading EC offset {offset:#04x}"))?;
        Ok(buf[0])
    }

    pub fn dump(&self) -> Result<[u8; EC_SIZE]> {
        let mut buf = [0u8; EC_SIZE];
        self.file
            .read_exact_at(&mut buf, 0)
            .context("reading EC RAM")?;
        Ok(buf)
    }
}
