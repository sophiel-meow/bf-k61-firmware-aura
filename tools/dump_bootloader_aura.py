#!/usr/bin/env python3
"""Dump the on-chip bootloader region from a radio running the Aura
firmware, over Aura's own CPS protocol.
"""

import argparse
import struct
import sys

import serial

HANDSHAKE = b"PROGRAMBF-K6AURA"
ACK = 0x06

FRAME_HEADER = 0xA6
CMD_READ_BOOT = 0x42
CMD_END = 0x45
CMD_ERROR = 0xEE

CHUNK = 32
BOOTLOADER_SIZE = 0x2000

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


def read_chunk(port: serial.Serial, offset: int) -> bytes:
    last_err = "(no attempts made)"
    for _ in range(MAX_RETRIES):
        try:
            port.reset_input_buffer()
            port.write(_encode_frame(CMD_READ_BOOT, offset, b""))
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


def dump(port_name: str, baud: int, out_path: str) -> None:
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

        print(f"dumping bootloader 0x0000-0x{BOOTLOADER_SIZE - 1:04X}...")
        data = bytearray(BOOTLOADER_SIZE)
        last_pct = -1
        for offset in range(0, BOOTLOADER_SIZE, CHUNK):
            data[offset : offset + CHUNK] = read_chunk(port, offset)
            pct = (offset + CHUNK) * 100 // BOOTLOADER_SIZE
            if pct != last_pct:
                print(f"\r    {pct}% (0x{offset:04X})", end="", flush=True)
                last_pct = pct
        print()

        print("ending session (radio will reboot)...")
        end_session(port)
    finally:
        port.close()

    with open(out_path, "wb") as f:
        f.write(data)
    print(f"saved {len(data)} bytes to {out_path}")


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("port", help="serial port, e.g. /dev/ttyUSB0")
    ap.add_argument(
        "out",
        nargs="?",
        default="bootloader_dump.bin",
        help="output file (default: bootloader_dump.bin)",
    )
    ap.add_argument("--baud", type=int, default=115200)
    args = ap.parse_args()

    try:
        dump(args.port, args.baud, args.out)
    except Exception as e:
        print(f"\n[ERR] {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
