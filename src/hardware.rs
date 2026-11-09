//! Machine identification and the sensors the research tools sample.

use std::fs;

/// DMI vendor and product names the EC layout was verified on.
const TESTED: &[(&str, &str)] = &[("TIMI", "Redmi Book Pro 15 2022")];

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}

pub fn warn_if_untested() {
    let vendor = read_trimmed("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let product = read_trimmed("/sys/class/dmi/id/product_name").unwrap_or_default();
    if !TESTED.iter().any(|&(v, p)| v == vendor && p == product) {
        eprintln!("warning: untested machine {vendor:?} {product:?}, the EC layout may differ");
    }
}

/// Package temperature from `coretemp`, in degrees Celsius.
pub fn cpu_temp() -> Option<u32> {
    for entry in fs::read_dir("/sys/class/hwmon").ok()? {
        let dir = entry.ok()?.path();
        if fs::read_to_string(dir.join("name")).ok()?.trim() == "coretemp" {
            let milli: u32 = fs::read_to_string(dir.join("temp1_input"))
                .ok()?
                .trim()
                .parse()
                .ok()?;
            return Some(milli / 1000);
        }
    }
    None
}

/// Average current clock over all CPUs, in MHz.
pub fn cpu_mhz() -> Option<u64> {
    let mut sum = 0u64;
    let mut count = 0u64;
    for entry in fs::read_dir("/sys/devices/system/cpu").ok()? {
        let path = entry.ok()?.path().join("cpufreq/scaling_cur_freq");
        if let Some(khz) = fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            sum += khz;
            count += 1;
        }
    }
    (count > 0).then(|| sum / count / 1000)
}

/// Cumulative CPU package energy from RAPL, in microjoules. Root only.
pub fn package_energy_uj() -> Option<u64> {
    read_trimmed("/sys/class/powercap/intel-rapl:0/energy_uj")?
        .parse()
        .ok()
}
