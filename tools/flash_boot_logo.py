#!/usr/bin/env python3
import argparse
import struct
import sys

try:
    import serial
except ImportError:
    print("[ERR] pyserial is required: pip install pyserial", file=sys.stderr)
    raise

try:
    from PIL import Image
except ImportError:
    print("[ERR] Pillow is required: pip install pillow", file=sys.stderr)
    raise

HANDSHAKE = b"PROGRAMBF-K6AURA"
ACK = 0x06

FRAME_HEADER = 0xA6
CMD_READ_LOGO = 0x4C
CMD_WRITE_LOGO = 0x6C
CMD_END = 0x45
CMD_ERROR = 0xEE

LCD_WIDTH = 128
LCD_HEIGHT = 64
LCD_PAGES = LCD_HEIGHT // 8
LOGO_SIZE = LCD_WIDTH * LCD_PAGES  # 1024 bytes

CHUNK = 64

HANDSHAKE_TIMEOUT_S = 1.0
HANDSHAKE_ATTEMPTS = 5
MAX_RETRIES = 5


def _crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if (crc & 0x8000) else (crc << 1)
        crc &= 0xFFFF
    return crc


def _encode_frame(cmd: int, addr: int, data: bytes) -> bytes:
    body = struct.pack(">BHH", cmd, addr, len(data)) + data
    crc = _crc16_xmodem(body)
    return bytes([FRAME_HEADER]) + body + struct.pack(">H", crc)


def _read_exact(port: serial.Serial, n: int) -> bytes:
    buf = bytearray()
    deadline_reads = 0
    while len(buf) < n:
        chunk = port.read(n - len(buf))
        if not chunk:
            deadline_reads += 1
            if deadline_reads > 40:  # ~2s at the 0.05s port timeout below
                raise RuntimeError("timed out waiting for a response from the radio")
            continue
        buf.extend(chunk)
    return bytes(buf)


def _recv_frame(port: serial.Serial):
    header = _read_exact(port, 6)
    if header[0] != FRAME_HEADER:
        raise RuntimeError("bad frame header from radio")
    cmd, addr, length = struct.unpack(">BHH", header[1:])
    data = _read_exact(port, length)
    crc_rx = struct.unpack(">H", _read_exact(port, 2))[0]
    if crc_rx != _crc16_xmodem(header[1:] + data):
        raise RuntimeError("CRC mismatch in response from radio")
    if cmd == CMD_ERROR:
        raise RuntimeError("radio reported protocol error 0x%02x" % data[0])
    return cmd, addr, data


def handshake(port: serial.Serial) -> None:
    port.timeout = HANDSHAKE_TIMEOUT_S
    port.reset_input_buffer()
    for _attempt in range(HANDSHAKE_ATTEMPTS):
        port.write(HANDSHAKE)
        port.flush()
        ack = port.read(1)
        if ack == bytes([ACK]):
            return
    raise RuntimeError(
        f"no handshake ACK from radio after {HANDSHAKE_ATTEMPTS} attempts -- "
        "is this an Aura-firmware radio, and is the cable/port correct?"
    )


def write_chunk(port: serial.Serial, offset: int, data: bytes) -> None:
    last_err = "(no attempts made)"
    for _ in range(MAX_RETRIES):
        try:
            port.reset_input_buffer()
            port.write(_encode_frame(CMD_WRITE_LOGO, offset, data))
            port.flush()
            _, addr, resp = _recv_frame(port)
        except RuntimeError as e:
            last_err = str(e)
            continue
        if addr != offset or len(resp) != 0:
            last_err = f"unexpected response: addr=0x{addr:04X} len={len(resp)}"
            continue
        return
    raise RuntimeError(
        f"write at offset 0x{offset:04X} failed after {MAX_RETRIES} retries: {last_err}"
    )


def read_chunk(port: serial.Serial, offset: int) -> bytes:
    last_err = "(no attempts made)"
    for _ in range(MAX_RETRIES):
        try:
            port.reset_input_buffer()
            port.write(_encode_frame(CMD_READ_LOGO, offset, b""))
            port.flush()
            _, addr, data = _recv_frame(port)
        except RuntimeError as e:
            last_err = str(e)
            continue
        if addr != offset or len(data) != CHUNK:
            last_err = f"unexpected response: addr=0x{addr:04X} len={len(data)}"
            continue
        return data
    raise RuntimeError(
        f"read at offset 0x{offset:04X} failed after {MAX_RETRIES} retries: {last_err}"
    )


def end_session(port: serial.Serial) -> None:
    port.write(_encode_frame(CMD_END, 0, b""))
    port.flush()
    try:
        _recv_frame(port)
    except RuntimeError:
        pass


def load_and_binarise(path: str, threshold, stretch: bool, invert: bool) -> Image.Image:
    img = Image.open(path).convert("L")

    if stretch:
        img = img.resize((LCD_WIDTH, LCD_HEIGHT), Image.LANCZOS)
    else:
        img = img.copy()
        img.thumbnail((LCD_WIDTH, LCD_HEIGHT), Image.LANCZOS)
        canvas = Image.new("L", (LCD_WIDTH, LCD_HEIGHT), 0)
        x = (LCD_WIDTH - img.width) // 2
        y = (LCD_HEIGHT - img.height) // 2
        canvas.paste(img, (x, y))
        img = canvas

    if threshold is None:
        img = img.convert("1")  # Pillow default: Floyd-Steinberg dither
    else:
        img = img.point(lambda p: 255 if p >= threshold else 0).convert("1")

    if invert:
        img = img.point(lambda p: 255 - p)

    return img


def pack_st7565(img: Image.Image) -> bytes:
    px = img.load()
    out = bytearray(LOGO_SIZE)
    for page in range(LCD_PAGES):
        for col in range(LCD_WIDTH):
            byte = 0
            for bit in range(8):
                y = page * 8 + bit
                if not px[col, y]:  # Pillow "1" mode: 0 = black = segment on
                    byte |= 1 << bit
            out[page * LCD_WIDTH + col] = byte
    return bytes(out)


def flash_logo(port_name: str, baud: int, image_bytes: bytes, verify: bool) -> None:
    assert len(image_bytes) == LOGO_SIZE

    port = serial.Serial(
        port_name,
        baudrate=baud,
        bytesize=serial.EIGHTBITS,
        parity=serial.PARITY_NONE,
        stopbits=serial.STOPBITS_ONE,
        timeout=0.05,
    )

    try:
        print("handshake...")
        handshake(port)

        print(
            f"writing boot logo (0-0x{LOGO_SIZE - 1:03X}, offset 0 erases the sector)..."
        )
        for offset in range(0, LOGO_SIZE, CHUNK):
            write_chunk(port, offset, image_bytes[offset : offset + CHUNK])
            pct = (offset + CHUNK) * 100 // LOGO_SIZE
            print(f"\r    {pct}%", end="", flush=True)
        print()

        if verify:
            print("verifying...")
            readback = bytearray(LOGO_SIZE)
            for offset in range(0, LOGO_SIZE, CHUNK):
                readback[offset : offset + CHUNK] = read_chunk(port, offset)
            if bytes(readback) != image_bytes:
                raise RuntimeError("readback does not match what was written")
            print("    OK")

        print("ending session (radio will reboot)...")
        end_session(port)
    finally:
        port.close()


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("port", help="serial port, e.g. /dev/ttyUSB0")
    ap.add_argument(
        "image", help="path to the source image (any Pillow-readable format)"
    )
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument(
        "--threshold",
        type=int,
        default=None,
        metavar="0-255",
        help="plain black/white cutoff instead of dithering (try 128 for line art/logos; "
        "omit for photos, which look better dithered)",
    )
    ap.add_argument(
        "--stretch",
        action="store_true",
        help="stretch the source image to fill 128x64 exactly, instead of "
        "fitting it centered with aspect ratio preserved (the default)",
    )
    ap.add_argument(
        "--invert", action="store_true", help="invert black/white after binarizing"
    )
    ap.add_argument(
        "--preview",
        metavar="FILE.png",
        help="also save the exact 128x64 1-bit image that will be sent, for a quick look "
        "before committing to flashing it",
    )
    ap.add_argument(
        "--no-verify",
        action="store_true",
        help="skip reading the image back after writing to confirm it matches",
    )
    args = ap.parse_args()

    try:
        img = load_and_binarise(args.image, args.threshold, args.stretch, args.invert)
        if args.preview:
            img.save(args.preview)
            print(f"preview saved to {args.preview}")
        image_bytes = pack_st7565(img)
        flash_logo(args.port, args.baud, image_bytes, verify=not args.no_verify)
    except Exception as e:
        print(f"\n[ERR] {e}", file=sys.stderr)
        return 1
    print("done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
