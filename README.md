# RedmiBook Fan Mode

On-screen notification of the fan mode when you press Fn+K on a Redmi Book Pro 15 2022 under Linux.

The embedded controller (EC) switches between Performance, Balanced and Silent on Fn+K by itself and never tells the OS. On Windows the vendor app shows the mode; on Linux nothing does, so a laptop left in Silent just runs hot with quiet fans. This daemon catches Fn+K, reads the new mode from EC memory and shows it as a desktop banner.

The EC is only read, never written.

## Install

Needs a kernel with `ec_sys` as a module (`CONFIG_ACPI_EC_DEBUGFS=m`, true for Ubuntu and most distros), systemd and a freedesktop notification server.

From a release (x86_64):

```
curl -LO https://github.com/0x57C0DE/RedmiBook-Fan-Mode/releases/latest/download/redmibook-fan-mode-x86_64-linux.tar.gz
tar xzf redmibook-fan-mode-x86_64-linux.tar.gz
cd redmibook-fan-mode-x86_64-linux
sudo make install
```

From source (Rust 1.88+):

```
make build
sudo make install
```

Press Fn+K: a banner shows the new mode for 3 seconds and is then removed from the notification list.

```
sudo redmibook-fan-mode status    # current mode and fan readings
sudo make uninstall
```

## Kernel 7.2+: bitland_mifs_wmi

Kernel 7.2 added `bitland_mifs_wmi`, which offers a `platform_profile`, but at least BIOS `RMAAD5B0P1010` rejects every write. `power-profiles-daemon` then rolls back each profile change in GNOME. To fix that:

```
sudo make install-workarounds     # undo: sudo make uninstall-workarounds
```

It blacklists the module and keeps `power-profiles-daemon` on CPU EPP only.

## Other models

Mode byte, fan bytes and the Fn+K scancode were mapped on one machine; see [docs/ec-map.md](docs/ec-map.md). Other Redmi or Xiaomi laptops may differ. The `record` and `diff` subcommands are the tools used for the mapping, so you can repeat it:

```
sudo redmibook-fan-mode record --seconds 60 --load --press-at 20 40 --out modes.jsonl
redmibook-fan-mode diff modes.jsonl --key 0xE4
```

## License

MIT
