#!/usr/bin/env python3
import argparse
import struct
import sys
import time

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
CMD_SSTV_ERASE = 0x53  # 'S'
CMD_SSTV_WRITE = 0x73  # 's'
CMD_END = 0x45  # 'E'
CMD_ERROR = 0xEE

SSTV_WIDTH = 320
SSTV_HEIGHT = 240
CHROMA_WIDTH = SSTV_WIDTH // 2  # 160
ROW_STRIDE = SSTV_WIDTH + CHROMA_WIDTH  # 480
SSTV_SIZE = ROW_STRIDE * SSTV_HEIGHT  # 115200 bytes

CHUNK = 64  # bytes per CPS frame
CHUNK_COUNT = SSTV_SIZE // CHUNK  # 1800 chunks

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
            if deadline_reads > 40:  # ~2s at 0.05s timeout
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


def erase_sstv(port: serial.Serial) -> None:
    """Send the SSTV erase command (CMD_SSTV_ERASE)."""
    for _ in range(MAX_RETRIES):
        try:
            port.reset_input_buffer()
            port.write(_encode_frame(CMD_SSTV_ERASE, 0, b""))
            port.flush()
            _, _, _ = _recv_frame(port)
            return
        except RuntimeError as e:
            last_err = str(e)
            continue
    raise RuntimeError(f"SSTV erase failed after {MAX_RETRIES} retries: {last_err}")


def write_chunk(port: serial.Serial, chunk_idx: int, data: bytes) -> None:
    """Write one 64-byte chunk at the given chunk index."""
    assert len(data) == CHUNK
    last_err = "(no attempts made)"
    for _ in range(MAX_RETRIES):
        try:
            port.reset_input_buffer()
            port.write(_encode_frame(CMD_SSTV_WRITE, chunk_idx, data))
            port.flush()
            _, addr, resp = _recv_frame(port)
        except RuntimeError as e:
            last_err = str(e)
            continue
        if addr != chunk_idx or len(resp) != 0:
            last_err = f"unexpected response: addr=0x{addr:04X} len={len(resp)}"
            continue
        return
    raise RuntimeError(
        f"write chunk {chunk_idx} failed after {MAX_RETRIES} retries: {last_err}"
    )


def end_session(port: serial.Serial) -> None:
    port.write(_encode_frame(CMD_END, 0, b""))
    port.flush()
    try:
        _recv_frame(port)
    except RuntimeError:
        pass


def _fit_to_canvas(img: Image.Image, stretch: bool) -> Image.Image:
    if stretch:
        return img.resize((SSTV_WIDTH, SSTV_HEIGHT), Image.LANCZOS)
    img = img.copy()
    img.thumbnail((SSTV_WIDTH, SSTV_HEIGHT), Image.LANCZOS)
    canvas = Image.new("RGB", (SSTV_WIDTH, SSTV_HEIGHT), (0, 0, 0))
    x = (SSTV_WIDTH - img.width) // 2
    y = (SSTV_HEIGHT - img.height) // 2
    canvas.paste(img, (x, y))
    return canvas


def load_and_convert(path: str, stretch: bool) -> bytes:
    img = Image.open(path).convert("RGB")
    img = _fit_to_canvas(img, stretch)
    ycbcr = img.convert("YCbCr")
    px = ycbcr.load()

    out = bytearray(SSTV_SIZE)
    for row in range(SSTV_HEIGHT):
        base = row * ROW_STRIDE
        chroma_idx = 2 if row % 2 == 0 else 1
        for col in range(SSTV_WIDTH):
            out[base + col] = px[col, row][0]
        for j in range(CHROMA_WIDTH):
            out[base + SSTV_WIDTH + j] = px[2 * j, row][chroma_idx]

    return bytes(out)


def upload_sstv(port_name: str, baud: int, image_bytes: bytes) -> None:
    assert (
        len(image_bytes) == SSTV_SIZE
    ), f"expected {SSTV_SIZE} bytes, got {len(image_bytes)}"

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

        print("erasing SSTV image sector...")
        erase_sstv(port)

        print(f"writing {SSTV_SIZE} bytes in {CHUNK_COUNT} chunks...")
        for chunk_idx in range(CHUNK_COUNT):
            offset = chunk_idx * CHUNK
            data = image_bytes[offset : offset + CHUNK]
            write_chunk(port, chunk_idx, data)
            pct = (chunk_idx + 1) * 100 // CHUNK_COUNT
            print(f"\r    {pct}%", end="", flush=True)
        print()

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
        "--stretch",
        action="store_true",
        help="stretch the source image to fill 320x240 exactly, instead of "
        "fitting it centered with aspect ratio preserved (the default)",
    )
    ap.add_argument(
        "--preview",
        metavar="FILE.png",
        help="also save an RGB preview of exactly what will be sent "
        "(Y/chroma round-tripped back through YCbCr->RGB), for a quick "
        "look before committing to flashing it",
    )
    args = ap.parse_args()

    try:
        image_bytes = load_and_convert(args.image, args.stretch)
        print(
            f"image: {SSTV_WIDTH}x{SSTV_HEIGHT}, {len(image_bytes)} bytes "
            f"({ROW_STRIDE} bytes/row: {SSTV_WIDTH} Y + {CHROMA_WIDTH} chroma)"
        )

        if args.preview:
            preview = Image.new("YCbCr", (SSTV_WIDTH, SSTV_HEIGHT))
            ppx = preview.load()
            for row in range(SSTV_HEIGHT):
                base = row * ROW_STRIDE
                chroma_idx = 2 if row % 2 == 0 else 1
                other_idx = 1 if chroma_idx == 2 else 2
                for col in range(SSTV_WIDTH):
                    y = image_bytes[base + col]
                    c = image_bytes[base + SSTV_WIDTH + col // 2]
                    px = [0, 0, 0]
                    px[0] = y
                    px[chroma_idx] = c
                    px[other_idx] = 128
                    ppx[col, row] = tuple(px)
            preview.convert("RGB").save(args.preview)
            print(f"preview saved to {args.preview}")

        upload_sstv(args.port, args.baud, image_bytes)
    except Exception as e:
        print(f"\n[ERR] {e}", file=sys.stderr)
        return 1

    print("done. Image uploaded -- use SSTV TX CQ/QSO to transmit.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
