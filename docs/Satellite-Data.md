# Importing Satellite Data

`tools/update_satellites.py` writes satellite pass data (frequencies, CTCSS tones, altitude) into the radio, for use with the satellite pass tracking / Doppler-corrected TX-RX feature.

> [!NOTE]
> This runs with the radio powered on and running Aura normally — not in `UPDATE` bootloader mode. The radio's screen switches to `AURA CPS PROGRAMMING...` while the tool is writing, then resets back to normal operation once it's done.

## Requirements

- Python 3 with [`pyserial`](https://pypi.org/project/pyserial/) installed:
  ```sh
  pip install pyserial
  ```
- The same USB-to-TTL programming cable used for flashing.

## Seeing what's available

The tool ships with a built-in list of amateur satellites (repeaters and SSTV-only downlinks). List them without connecting to a radio:

```sh
python tools/update_satellites.py --list
```

## Basic usage: write the default set

With no extra options, this writes a sensible default set of satellites (ISS, SO-50, AO-91, AO-123, CAS-3H, IO-86, PO-101, RS95S, ISS SSTV):

```sh
python tools/update_satellites.py /dev/ttyUSB0
```

On Windows, replace `/dev/ttyUSB0` with the COM port (e.g. `COM3`).

## Choosing which satellites to write

```sh
python tools/update_satellites.py /dev/ttyUSB0 --sat ISS SO-50 "ISS SSTV"
```

Or write everything the tool knows about (up to the radio's 20-slot limit):

```sh
python tools/update_satellites.py /dev/ttyUSB0 --all-known
```

### Options

| Option | Meaning |
|---|---|
| `--sat NAME [NAME ...]` | Specific satellites to write (see `--list` for valid names). Defaults to a built-in set of 9 popular ones if omitted. |
| `--all-known` | Write every built-in satellite instead of just the defaults. |
| `--tle` | Fetch live TLE data from [Celestrak](https://celestrak.org) to compute accurate current altitudes for the selected satellites, instead of using the built-in default altitude estimates. |
| `--tle-file PATH` | Same as `--tle`, but read TLE data from a local file instead of fetching it online (useful if you're offline, or already have a TLE file from another source). |
| `--rx-only` | Clear all TX frequencies, turning every selected satellite into receive-only. |
| `--baudrate BAUD` | Serial baud rate. Default `115200` — you shouldn't need to change this. |
| `--dry-run` | Print exactly what would be written, without connecting to the radio at all — useful to sanity-check your selection first. |

### Checking your selection first

Before actually writing to the radio, it's worth a dry run:

```sh
python tools/update_satellites.py --dry-run --sat ISS SO-50 --tle
```

This prints the resolved satellite records (frequencies, tones, altitude) without touching the radio.

## Note on altitude and TLE data

Each satellite record includes an altitude, used for Doppler correction math. Without `--tle`/`--tle-file`, the tool uses a fixed default altitude baked into its built-in table — accurate enough for most passes, but if you want the current, real orbital altitude (satellites drift over time, especially in low orbits), pass `--tle` to fetch fresh data from Celestrak, or `--tle-file` if you've saved a TLE file yourself.

## After writing

Once the write finishes and the radio resets, the satellites you wrote are available in the satellite pass tracking feature.
