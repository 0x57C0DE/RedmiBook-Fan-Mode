//! Shows the embedded controller fan mode of a Redmi Book Pro 15 2022 when
//! Fn+K switches it. See README.md for the background.

mod daemon;
mod ec;
mod hardware;
mod keyboard;
mod mode;
mod notify;
mod research;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::ec::{Ec, FAN_OFFSETS, MODE_OFFSET};
use crate::mode::Mode;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Watch Fn+K and show the new mode as a desktop banner (system service, root).
    Daemon,
    /// Print the current mode and fan readings (root).
    Status,
    /// Record EC snapshots and Fn+K presses to a JSON Lines file (root).
    Record {
        /// Recording length in seconds.
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
        /// Seconds between EC snapshots.
        #[arg(long, default_value_t = 2.0)]
        interval: f64,
        /// Keep every CPU busy while recording.
        #[arg(long)]
        load: bool,
        /// Print a reminder 3 s before each of these marks (seconds).
        #[arg(long, num_args = 1..)]
        press_at: Vec<f64>,
        /// Output file.
        #[arg(long)]
        out: PathBuf,
    },
    /// Find EC bytes that change in a recording.
    Diff {
        recording: PathBuf,
        /// Group snapshots by this byte (for example 0xE4) and rank the others.
        #[arg(long, value_parser = parse_offset)]
        key: Option<u8>,
        /// How many ranked bytes to print.
        #[arg(long, default_value_t = 15)]
        top: usize,
    },
    /// Internal: show one banner in the current session (run by the daemon as the user).
    #[command(hide = true)]
    Notify {
        #[arg(long)]
        mode: u8,
        #[arg(long, default_value_t = 0)]
        replaces: u32,
    },
}

fn parse_offset(s: &str) -> Result<u8, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u8::from_str_radix(hex, 16),
        None => s.parse(),
    };
    parsed.map_err(|e| format!("{s:?} is not an offset 0..=0xff: {e}"))
}

fn require_root() -> Result<()> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        bail!("needs root: the EC is only readable by root");
    }
    Ok(())
}

fn status() -> Result<()> {
    let ec = Ec::open()?;
    let raw = ec.read_byte(MODE_OFFSET)?;
    println!("mode: {} (raw {raw})", Mode::from_raw(raw));
    let fans: Vec<String> = FAN_OFFSETS
        .iter()
        .map(|&offset| ec.read_byte(offset).map(|v| format!("{offset:#04x}={v}")))
        .collect::<Result<_>>()?;
    println!("fans: {}", fans.join(" "));
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Daemon => {
            require_root()?;
            daemon::run()
        }
        Command::Status => {
            require_root()?;
            status()
        }
        Command::Record {
            seconds,
            interval,
            load,
            press_at,
            out,
        } => {
            require_root()?;
            research::record(&research::RecordOptions {
                seconds,
                interval,
                load,
                press_at: &press_at,
                out: &out,
            })
        }
        Command::Diff {
            recording,
            key,
            top,
        } => research::diff(&recording, key, top),
        Command::Notify { mode, replaces } => {
            notify::run_banner(Mode::from_raw(mode), replaces, notify::BANNER_TIME)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_parse_in_hex_and_decimal() {
        assert_eq!(parse_offset("0xE4"), Ok(0xE4));
        assert_eq!(parse_offset("228"), Ok(228));
        assert!(parse_offset("0x100").is_err());
    }
}
