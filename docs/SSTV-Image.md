# Importing an SSTV Image

`tools/sstv_upload.py` uploads a still image into the radio's SSTV image slot, so it can later be sent with the SSTV TX CQ/QSO function.

> [!NOTE]
> This runs with the radio powered on and running Aura normally — not in `UPDATE` bootloader mode. The radio's screen switches to `AURA CPS PROGRAMMING...` while the tool is writing, then resets back to normal operation once it's done.

## Requirements

- Python 3 with [`pyserial`](https://pypi.org/project/pyserial/) and [`Pillow`](https://pypi.org/project/pillow/) installed:
  ```sh
  pip install pyserial pillow
  ```
- The same USB-to-TTL programming cable used for flashing.
- Any image file Pillow can open (JPEG, PNG, etc.) that you'd like to transmit over SSTV.

## Usage

```sh
python tools/sstv_upload.py <port> <image>
```

For example:

```sh
python tools/sstv_upload.py /dev/ttyUSB0 photo.jpg
```

On Windows, replace `/dev/ttyUSB0` with the COM port (e.g. `COM3`).

While the upload is running, the radio's screen switches to `AURA CPS PROGRAMMING...` — this is expected. Once the transfer finishes, the radio resets on its own and returns to normal operation.

### Options

| Option | Meaning |
|---|---|
| `--baud BAUD` | Serial baud rate. Default `115200` — you shouldn't need to change this. |
| `--stretch` | Stretch the image to fill the full 320×240 frame exactly, ignoring aspect ratio. By default, the image is fit centered with aspect ratio preserved (letterboxed on black instead). |
| `--preview FILE.png` | Also save a PNG showing exactly what will be transmitted (after the image is round-tripped through the same Y/chroma encoding the radio uses), so you can check it looks right *before* spending time uploading it. |

### Previewing before uploading

The SSTV image is stored at reduced chroma resolution (luma at full 320×240, colour subsampled), so it's worth previewing what will actually be sent:

```sh
python tools/sstv_upload.py /dev/ttyUSB0 photo.jpg --preview preview.png
```

This writes `preview.png` so you can check it before committing — if it doesn't look right, cancel and adjust the source image.

## After uploading

Once the upload finishes and the radio reboots, the image is stored in flash and ready to transmit — use the SSTV TX CQ/QSO function on the radio to send it as a Robot 36 colour SSTV image.
