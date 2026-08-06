#!/usr/bin/env python3
import argparse
import struct
import sys
import time

import serial

HANDSHAKE = b"PROGRAMBF-K6AURA"
ACK = 0x06

FRAME_HEADER = 0xA6
CMD_READ_FLASH_RAW = 0x44  # 'D'
CMD_END = 0x45  # 'E'
CMD_ERROR = 0xEE

CHUNK = 64

CHIP_SIZE = 2 * 1024 * 1024
TOTAL_BLOCKS = CHIP_SIZE // CHUNK

BYTE_TIMEOUT = 2.0
MAX_RETRIES = 5


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if (crc & 0x8000) else (crc << 1)
            crc &= 0xFFFF
    return crc


def encode_frame(cmd: int, addr: int, data: bytes) -> bytes:
    body = struct.pack(">BHH", cmd, addr, len(data)) + data
    crc = crc16_xmodem(body)
    return bytes([FRAME_HEADER]) + body + struct.pack(">H", crc)


def read_exact(port: serial.Serial, n: int, timeout: float = BYTE_TIMEOUT) -> bytes:
    buf = bytearray()
    deadline = time.monotonic() + timeout
    while len(buf) < n and time.monotonic() < deadline:
        chunk = port.read(n - len(buf))
        if chunk:
            buf.extend(chunk)
    return bytes(buf)


def read_response(port: serial.Serial, expect_data_len):
    header = read_exact(port, 6)
    if len(header) != 6 or header[0] != FRAME_HEADER:
        raise RuntimeError(f"bad response header: {header.hex() or '(nothing)'}")
    cmd, addr, length = struct.unpack(">BHH", header[1:])
    rest = read_exact(port, length + 2)
    if len(rest) != length + 2:
        raise RuntimeError(f"short response body: {len(rest)}/{length + 2} bytes")

    data = rest[:length]
    got_crc = struct.unpack(">H", rest[length : length + 2])[0]
    if crc16_xmodem(header[1:] + data) != got_crc:
        raise RuntimeError("CRC mismatch in response")

    if cmd == CMD_ERROR:
        raise RuntimeError(
            f"radio reported protocol error 0x{data[0] if data else 0:02X}"
        )

    if expect_data_len is not None and length != expect_data_len:
        raise RuntimeError(
            f"unexpected data length: got {length}, wanted {expect_data_len}"
        )

    return addr, data


def send_and_expect(
    port: serial.Serial, cmd: int, addr: int, data: bytes, expect_data_len
):
    last_err = "(no attempts made)"
    for _ in range(MAX_RETRIES):
        port.reset_input_buffer()
        port.write(encode_frame(cmd, addr, data))
        port.flush()
        try:
            return read_response(port, expect_data_len)
        except RuntimeError as e:
            last_err = str(e)
    raise RuntimeError(
        f"cmd=0x{cmd:02X} addr=0x{addr:04X} failed after {MAX_RETRIES} retries: {last_err}"
    )


def handshake(port: serial.Serial) -> None:
    port.reset_input_buffer()
    port.write(HANDSHAKE)
    port.flush()
    resp = read_exact(port, 1)
    if resp != bytes([ACK]):
        raise RuntimeError(
            f"handshake failed: expected ACK 0x{ACK:02X}, got {resp.hex() or '(nothing)'}"
            " -- is this an Aura-firmware radio, and is the cable/port correct?"
        )


def dump(
    port_name: str, baud: int, out_path: str, start_block: int, end_block: int
) -> None:
    port = serial.Serial(
        port_name,
        baudrate=baud,
        bytesize=serial.EIGHTBITS,
        parity=serial.PARITY_NONE,
        stopbits=serial.STOPBITS_ONE,
        timeout=0.05,
    )
    port.dtr = True
    port.rts = True

    out = bytearray()
    try:
        print("handshake...")
        handshake(port)

        start_addr = start_block * CHUNK
        end_addr = end_block * CHUNK
        print(
            f"dumping 0x{start_addr:06X}-0x{end_addr:06X} ({end_block - start_block} blocks of {CHUNK}B)..."
        )
        last_pct = -1
        for block in range(start_block, end_block):
            _, data = send_and_expect(port, CMD_READ_FLASH_RAW, block, b"", CHUNK)
            out.extend(data)

            pct = (block - start_block + 1) * 100 // (end_block - start_block)
            if pct != last_pct:
                print(
                    f"\r    {pct}% (0x{start_addr + len(out):06X})", end="", flush=True
                )
                last_pct = pct
        print()

        print("ending session...")
        send_and_expect(port, CMD_END, 0, b"", 0)
    finally:
        port.close()

    with open(out_path, "wb") as f:
        f.write(out)
    print(
        f"saved {len(out)} bytes (0x{start_addr:06X}-0x{start_addr + len(out):06X}) to {out_path}"
    )


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("port", help="serial port, e.g. /dev/ttyUSB0")
    ap.add_argument("out", nargs="?", default="aura_spiflash_dump.bin")
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument(
        "--start",
        type=lambda s: int(s, 0),
        default=0,
        help="start physical address, rounded down to a block (default 0)",
    )
    ap.add_argument(
        "--end",
        type=lambda s: int(s, 0),
        default=CHIP_SIZE,
        help=f"stop physical address, exclusive, rounded up to a block "
        f"(default 0x{CHIP_SIZE:X} = full XM25QH16C)",
    )
    args = ap.parse_args()

    start_block = args.start // CHUNK
    end_block = (args.end + CHUNK - 1) // CHUNK
    if not (0 <= start_block < end_block <= TOTAL_BLOCKS):
        print(
            f"[ERR] address range out of bounds (chip is 0x{CHIP_SIZE:X} bytes)",
            file=sys.stderr,
        )
        return 1

    try:
        dump(args.port, args.baud, args.out, start_block, end_block)
    except Exception as e:
        print(f"\n[ERR] {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
