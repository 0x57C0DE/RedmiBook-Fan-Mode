//! Tools used to map the EC: record snapshots around Fn+K presses, then rank
//! the bytes that follow a chosen key byte.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::ec::{EC_SIZE, Ec};
use crate::hardware::{cpu_mhz, cpu_temp, package_energy_uj};
use crate::keyboard::Keyboard;

pub struct RecordOptions<'a> {
    pub seconds: f64,
    pub interval: f64,
    pub load: bool,
    pub press_at: &'a [f64],
    pub out: &'a Path,
}

/// Writes JSON Lines: `{"t","temp","mhz","watts","ec"}` snapshots with the EC
/// RAM as hex, and `{"t","press":true}` for every Fn+K press.
pub fn record(opts: &RecordOptions) -> Result<()> {
    let ec = Ec::open()?;
    let mut keyboard = Keyboard::open()?;
    let file =
        File::create(opts.out).with_context(|| format!("creating {}", opts.out.display()))?;
    // Recordings are made as root; keep them readable for the analysis step.
    fs::set_permissions(opts.out, fs::Permissions::from_mode(0o644))?;
    let mut out = BufWriter::new(file);

    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = if opts.load {
        let n = thread::available_parallelism().map_or(1, |n| n.get());
        (0..n)
            .map(|_| {
                let stop = Arc::clone(&stop);
                thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::hint::spin_loop();
                    }
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    println!(
        "recording {} s{}",
        opts.seconds,
        if opts.load { " under full load" } else { "" }
    );

    let start = Instant::now();
    let total = Duration::from_secs_f64(opts.seconds);
    let interval = Duration::from_secs_f64(opts.interval);
    let mut reminded = vec![false; opts.press_at.len()];
    let mut next_snapshot = start;
    let mut last_press: Option<Instant> = None;
    let mut energy = (package_energy_uj(), Instant::now());

    while start.elapsed() < total {
        let pressed = keyboard.wait_fn_k(Some(Duration::from_millis(200)))?;
        let t = start.elapsed().as_secs_f64();
        if pressed && last_press.is_none_or(|p| p.elapsed() > Duration::from_secs(1)) {
            last_press = Some(Instant::now());
            writeln!(out, "{}", json!({"t": round1(t), "press": true}))?;
            println!("{t:5.1}s Fn+K");
        }
        for (i, &mark) in opts.press_at.iter().enumerate() {
            if !reminded[i] && t >= mark - 3.0 {
                reminded[i] = true;
                println!("--- press Fn+K in 3 s (at {mark} s) ---");
            }
        }
        if Instant::now() >= next_snapshot {
            let now = Instant::now();
            let new_energy = package_energy_uj();
            let watts = match (energy.0, new_energy) {
                (Some(a), Some(b)) if b >= a && now > energy.1 => Some(round1(
                    (b - a) as f64 / 1e6 / (now - energy.1).as_secs_f64(),
                )),
                _ => None,
            };
            energy = (new_energy, now);
            let (temp, mhz) = (cpu_temp(), cpu_mhz());
            let record = json!({
                "t": round1(t), "temp": temp, "mhz": mhz, "watts": watts,
                "ec": to_hex(&ec.dump()?),
            });
            writeln!(out, "{record}")?;
            out.flush()?;
            println!("{t:5.1}s temp={temp:?} mhz={mhz:?} W={watts:?}");
            next_snapshot = now + interval;
        }
    }
    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        let _ = worker.join();
    }
    out.flush()?;
    println!("saved {}", opts.out.display());
    Ok(())
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Result<[u8; EC_SIZE]> {
    if hex.len() != EC_SIZE * 2 {
        bail!(
            "EC dump has {} hex digits, expected {}",
            hex.len(),
            EC_SIZE * 2
        );
    }
    let mut out = [0u8; EC_SIZE];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

struct Recording {
    presses: Vec<f64>,
    snapshots: Vec<(f64, [u8; EC_SIZE])>,
}

fn load(path: &Path) -> Result<Recording> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut rec = Recording {
        presses: Vec::new(),
        snapshots: Vec::new(),
    };
    for (n, line) in BufReader::new(file).lines().enumerate() {
        let value: Value =
            serde_json::from_str(&line?).with_context(|| format!("line {}", n + 1))?;
        let t = value["t"].as_f64().unwrap_or_default();
        if let Some(hex) = value["ec"].as_str() {
            rec.snapshots.push((t, from_hex(hex)?));
        } else if !value["press"].is_null() {
            rec.presses.push(t);
        }
    }
    Ok(rec)
}

/// Each change of one byte over time, as `t:value` pairs.
fn changes(snapshots: &[(f64, [u8; EC_SIZE])], offset: usize) -> String {
    let mut out = Vec::new();
    let mut previous = None;
    for (t, dump) in snapshots {
        if previous != Some(dump[offset]) {
            out.push(format!("{t}:{:02x}", dump[offset]));
            previous = Some(dump[offset]);
        }
    }
    out.join(" ")
}

/// Without `key`, lists every byte that changed. With `key`, groups snapshots
/// by that byte and ranks the other bytes by the spread of their group means.
pub fn diff(path: &Path, key: Option<u8>, top: usize) -> Result<()> {
    let rec = load(path)?;
    println!(
        "presses at {:?}, {} snapshots",
        rec.presses,
        rec.snapshots.len()
    );
    let changing: Vec<usize> = (0..EC_SIZE)
        .filter(|&i| rec.snapshots.windows(2).any(|w| w[0].1[i] != w[1].1[i]))
        .collect();

    let Some(key) = key.map(usize::from) else {
        for offset in changing {
            println!("{offset:#04x} {}", changes(&rec.snapshots, offset));
        }
        return Ok(());
    };

    println!("key {key:#04x}: {}", changes(&rec.snapshots, key));
    let mut ranked: Vec<(f64, usize, BTreeMap<u8, f64>)> = changing
        .into_iter()
        .filter(|&offset| offset != key)
        .map(|offset| {
            let mut groups: BTreeMap<u8, (f64, u32)> = BTreeMap::new();
            for (_, dump) in &rec.snapshots {
                let group = groups.entry(dump[key]).or_default();
                group.0 += f64::from(dump[offset]);
                group.1 += 1;
            }
            let means: BTreeMap<u8, f64> = groups
                .into_iter()
                .map(|(k, (sum, n))| (k, sum / f64::from(n)))
                .collect();
            let max = means.values().copied().fold(f64::MIN, f64::max);
            let min = means.values().copied().fold(f64::MAX, f64::min);
            (max - min, offset, means)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (spread, offset, means) in ranked.into_iter().take(top) {
        let shown: Vec<String> = means.iter().map(|(k, m)| format!("{k}={m:.1}")).collect();
        println!("{offset:#04x} spread={spread:.1} {}", shown.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let mut dump = [0u8; EC_SIZE];
        for (i, b) in dump.iter_mut().enumerate() {
            *b = i as u8;
        }
        assert_eq!(from_hex(&to_hex(&dump)).unwrap(), dump);
    }

    #[test]
    fn rejects_short_dump() {
        assert!(from_hex("00ff").is_err());
    }
}
