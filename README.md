# Aura — Open-source Rust firmware for Baofeng UV-K61

**English** | [简体中文](README.zh.md)

Aura is a clean-room Rust firmware for the Baofeng UV-K61 series of handheld radios (KD32F328CB MCU).

> [!WARNING]
> Use this firmware entirely at your own risk. There is absolutely no guarantee that it will work on your radio — it may even brick it, in which case you'd need to replace the device. That said, have fun.

## Wide / Out-of-Band Transmit — Read Before Use

Aura includes a TX frequency lock setting (`FLOCK` in the settings menu). When set to `Unlocked`, the radio can transmit across the full hardware synthesizer range, rather than being restricted to amateur radio allocations.

This is an experimental capability that most users don't need:

- Transmitting outside your licensed bands is almost certainly illegal in your jurisdiction.
- Driving the PA at frequencies it wasn't calibrated for can damage the radio.

Know and follow your local radio regulations, and hold the appropriate license before transmitting outside your amateur allocation.

## Features

- Dual VFO / channel operation with CTCSS, DCS, VOX, split-frequency, and all standard handheld radio functions
- CW mode (manual keying via PTT or external key)
- APRS beacon (AX.25 AFSK) with configurable path, symbol, and comment fields
- Spectrum
- Robot 36 colour SSTV transmit
- Satellite pass tracking with Doppler-corrected TX/RX
- Customisable boot logo and boot melody
- CPS (programming) support via a custom wire protocol and a CHIRP driver

## Screenshots

<table>
<tr>
<td width="33%"><img src="assets/main.webp" alt="Home screen with dual watch"><br>Home screen — dual watch (memory + VFO)</td>
<td width="33%"><img src="assets/menu.webp" alt="App menu"><br>App menu</td>
<td width="33%"><img src="assets/spectrum.webp" alt="Spectrum scan"><br>Spectrum scan</td>
</tr>
<tr>
<td width="33%"><img src="assets/sat.webp" alt="Satellite tracking"><br>Satellite pass tracking (Doppler-corrected)</td>
<td width="33%"><img src="assets/aprs.webp" alt="APRS digipeater path settings"><br>APRS TX</td>
<td width="33%"><img src="assets/sstv.webp" alt="SSTV CQ detail entry"><br>SSTV (Robot 36 Colour) TX</td>
</tr>
</table>

## Relationship to the Factory Firmware

Baofeng publishes source-available firmware for the UV-K6x series at [cnt7/BAOFENG-UV-K6-Firmware](https://github.com/cnt7/BAOFENG-UV-K6-Firmware), under the Baofeng Public License (BFPL-1.0) — a custom non-free license that does not permit redistributing modified or derivative code under a free license.

Aura does not use, copy, or derive from any of that code. Every line is an independent, clean-room implementation written from scratch. On-air behaviour and on-flash layout were reverse-engineered only where necessary for hardware compatibility, and those cases are documented inline. Aura's CPS wire protocol is also deliberately incompatible with the factory one: different handshake string, different framing.

Thanks to Baofeng and the contributors to that repository for making the source available to study.

## Hardware

- **MCU:** KD32F328CB (ARM Cortex-M0)
- **Flash:** XM25QH16C SPI NOR (2 MB)
- **RF / baseband:** FD6818
- **Display controller:** SC5260

## Building

With [Nix](https://nixos.org) (recommended — pins the exact toolchain):

```sh
nix develop
cargo build --release
```

Without Nix: install Rust via [rustup](https://rustup.rs). The pinned stable channel and `thumbv6m-none-eabi` target are picked up automatically from `rust-toolchain.toml`:

```sh
cargo build --release
cargo objcopy --release -- -O binary aura.bin
```

Or, equivalently, `make bin`.

## Flashing

Aura is flashed over the stock UV-K6x UART bootloader — the same mechanism the factory firmware uses — not via SWD or DFU. See the [Flashing Guide](docs/Flashing-Guide.md) for the full walkthrough, including how to back up SPI flash first.

The quickest path is the Python tool in `tools/`:

```sh
pip install pyserial
python tools/flash.py aura.bin /dev/ttyUSB0
```

No Python? Use [BF-K6x-flash](https://github.com/sophiel-meow/BF-K6x-flash), a Rust CLI reimplementation of the same protocol:

```sh
git clone https://github.com/sophiel-meow/BF-K6x-flash tools/flash-rs
cargo build --release --manifest-path tools/flash-rs/Cargo.toml
make flash   # or: ./tools/flash-rs/target/release/flash aura.bin /dev/ttyUSB0
```

Want a GUI instead of a command line? The factory's official flashing tool, [`BFK6_Bootloader.exe`](https://github.com/cnt7/BAOFENG-UV-K6-Firmware/blob/main/BFK6_Bootloader.exe), is compatible with Aura's bootloader too. It's Windows-only and closed-source (so it's linked here rather than bundled), but it's the simplest option if you just want a "Browse... → Flash" experience.

## Programming (CPS)

Channels, memories, and radio settings can be programmed from [CHIRP](https://chirpmyradio.com/) using `tools/chirp_bfk6_aura.py` — Aura's CHIRP driver.

CHIRP only loads third-party driver modules like this one with **Developer Mode** enabled:

1. In CHIRP, enable it via **Help → Developer Mode**.
2. With Developer Mode on, use CHIRP's module loading option (under the **File** menu) to load `tools/chirp_bfk6_aura.py` directly — no installation elsewhere is needed.
3. When selecting the radio, choose **Vendor: Baofeng**, **Model: UV-K6 (Aura)**.

> [!NOTE]
> The stock `UV-K6` model entry in CHIRP won't work with Aura — the CPS wire protocol is deliberately incompatible with the factory one. Always pick the `UV-K6 (Aura)` model.

For flashing firmware, backing up SPI flash, importing SSTV images / satellite data / a boot logo, and a full rundown of every settings menu entry, see: [Flashing Guide](docs/Flashing-Guide.md), [SSTV Image](docs/SSTV-Image.md), [Satellite Data](docs/Satellite-Data.md), [Boot Logo](docs/Boot-Logo.md), [Settings Menu Reference](docs/Settings-Menu.md).

## License

Aura's own code is licensed under the GNU General Public License v3.0 — see [`LICENSE`](LICENSE).

`kd32f328-pac/`, the peripheral access crate generated from the KD32F328CB SVD, is licensed separately under Apache-2.0 — see [`kd32f328-pac/LICENSE`](kd32f328-pac/LICENSE). The SVD was authored by Amo Xu (BD4VOW).

## Acknowledgments

- Amo Xu (BD4VOW) for the KD32F328CB SVD
- egzumer, f4hwn, and losehu — their Quansheng UV-K5 firmware implementations were an invaluable reference for open-source radio firmware design
- Baofeng for their source-available firmware and docs

