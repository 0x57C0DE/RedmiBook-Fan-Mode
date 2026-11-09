# EC map: Redmi Book Pro 15 2022

Mapped on DMI `TIMI` / `Redmi Book Pro 15 2022`, BIOS `RMAAD5B0P1010`, kernel 7.2. Read-only findings.

## Access

`ec_sys` exposes 256 bytes of EC RAM at `/sys/kernel/debug/ec/ec0/io` (root). Use positional or unbuffered reads: a buffered reader returns stale data after `seek`.

## Fn+K

Handled by the EC itself. Linux only gets an unmapped scancode on `AT Translated Set 2 keyboard`: `EV_MSC`/`MSC_SCAN` `0x86` four times, then `0xF6`. No key event, nothing on `Redmibook WMI keys`. The mode byte often changes before the scancode arrives.

## Offsets

| Offset | Meaning | Values |
|---|---|---|
| `0xE4` | fan and power mode | `0` Performance, `1` Balanced, `2` Silent. Fn+K cycles 1, 0, 2 |
| `0x9B`, `0x9D` | fan speed, one per fan | full load: about 20 Balanced, 22 Performance, down to 15 Silent. Unit unknown, likely x100 rpm |

Under sustained full load the CPU hits its thermal limit (about 96 C, 47 W) in every mode, so the mode mostly changes fan noise, not sustained power.

## How it was found

1. `record` EC snapshots while pressing Fn+K, then `diff` to list changed bytes. `0xE4` changed exactly at each press and cycled through three values.
2. `record --load` with about 20 s per mode, then `diff --key 0xE4` to rank bytes by how they differ between modes. The fan bytes follow the mode slowly.

Bytes `0x6C`, `0x70`, `0x9C`, `0x9E` change on every snapshot and are noise for this purpose.

## bitland_mifs_wmi

On this BIOS the driver's `platform_profile` writes fail with `Operation not supported`, reads always return `balanced`, and its hwmon fans read `0 RPM`.

## Open

- Fan speed unit; meaning of `0x8A`, `0x8C`, `0x8D`.
- Whether writing `0xE4` sets the mode (not tried on purpose).
