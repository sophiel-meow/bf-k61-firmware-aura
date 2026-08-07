# Importing a Boot Logo

`tools/flash_boot_logo.py` converts an image into a 128×64 monochrome bitmap and flashes it as the radio's custom boot screen.

> [!NOTE]
> This runs with the radio powered on and running Aura normally — not in `UPDATE` bootloader mode. The radio's screen switches to `AURA CPS PROGRAMMING...` while the tool is writing, then resets back to normal operation once it's done.

## Requirements

- Python 3 with [`pyserial`](https://pypi.org/project/pyserial/) and [`Pillow`](https://pypi.org/project/pillow/) installed:
  ```sh
  pip install pyserial pillow
  ```
- The same USB-to-TTL programming cable used for flashing.
- A source image. Since the display is 128×64 pure black/white (no greyscale), simple high-contrast line art or a wordmark works much better than a photo.

## Usage

```sh
python tools/flash_boot_logo.py <port> <image>
```

For example:

```sh
python tools/flash_boot_logo.py /dev/ttyUSB0 logo.png
```

On Windows, replace `/dev/ttyUSB0` with the COM port (e.g. `COM3`).

By default, the tool reads the image back after writing to confirm it landed correctly.

### Options

| Option | Meaning |
|---|---|
| `--threshold 0-255` | Convert to black/white with a plain brightness cutoff instead of dithering. Try `128` for line art or logos — dithering (the default when this is omitted) tends to look better on photos, but noisy on simple graphics. |
| `--stretch` | Stretch the source image to fill 128×64 exactly, instead of fitting it centered with aspect ratio preserved (the default). |
| `--invert` | Invert black/white after conversion. |
| `--preview FILE.png` | Save the exact 128×64 1-bit image that will be sent, so you can check it looks right *before* flashing it. |
| `--baud BAUD` | Serial baud rate. Default `115200` — you shouldn't need to change this. |
| `--no-verify` | Skip reading the image back after writing to confirm it matches (the default is to verify). |

### Previewing before flashing

Since the display has no greyscale, it's worth checking the converted result before spending time uploading it:

```sh
python tools/flash_boot_logo.py /dev/ttyUSB0 logo.png --threshold 128 --preview preview.png
```

Open `preview.png` and check it's legible at 128×64 before proceeding — if it isn't, try `--invert`, adjust `--threshold`, or start from cleaner source art.

## Enabling the logo on boot

Uploading the bitmap alone doesn't switch the radio's boot screen to show it — that's a separate on-device setting. After flashing:

1. On the radio, go to the settings menu and find **`SYSTEM -> BOOTSCR`**.
2. Change it from `NONE`/`VOLT`/`MSG` to **`LOGO`**.

Power-cycle the radio to see it.
