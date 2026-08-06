#!/usr/bin/env python3
import argparse
import math
import struct
import sys
import time
from pathlib import Path
from urllib.request import urlopen

HEADER = 0xA6
CMD_READ = 0x52  # 'R'
CMD_WRITE = 0x57  # 'W'
CMD_END = 0x45  # 'E'
MAX_DATA = 96

HANDSHAKE = b"PROGRAMBF-K6AURA"
ACK = 0x06

MAX_SATELLITES = 20
SAT_RECORD_SIZE = 32
SAT_CPS_BASE = 0xD000

KNOWN_SATELLITES: dict[str, dict] = {
    "ISS": {
        "name": "ISS",
        "rx_freq_hz": 437_800_000,  # Downlink
        "tx_freq_hz": 145_990_000,  # Uplink (67.0 Hz CTCSS)
        "rx_tone_hz": 0,  # No RX tone needed
        "tx_tone_hz": 670,  # 67.0 Hz CTCSS
        "altitude_km": 420,
        "norad_id": 25544,
    },
    "SO-50": {
        "name": "SO-50",
        "rx_freq_hz": 436_795_000,  # Downlink
        "tx_freq_hz": 145_850_000,  # Uplink (67.0 + 74.4 Hz arm)
        "rx_tone_hz": 0,
        "tx_tone_hz": 670,  # 67.0 Hz (after 74.4 Hz arm)
        "altitude_km": 620,
        "norad_id": 27607,
    },
    "AO-91": {
        "name": "AO-91",
        "rx_freq_hz": 145_960_000,  # Downlink (VHF)
        "tx_freq_hz": 435_250_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 670,  # 67.0 Hz CTCSS (may work carrier-operated)
        "altitude_km": 460,
        "norad_id": 43017,
    },
    "AO-123": {
        "name": "AO-123",
        "rx_freq_hz": 435_400_000,  # Downlink
        "tx_freq_hz": 145_850_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 670,  # 67.0 Hz CTCSS
        "altitude_km": 500,
        "norad_id": 58646,
    },
    "CAS-3H": {
        "name": "CAS-3H",
        "rx_freq_hz": 437_200_000,  # Downlink (also telemetry beacon)
        "tx_freq_hz": 144_350_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,  # No CTCSS listed
        "altitude_km": 480,
        "norad_id": 42759,
    },
    "IO-86": {
        "name": "IO-86",
        "rx_freq_hz": 435_880_000,  # Downlink
        "tx_freq_hz": 145_880_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 885,  # 88.5 Hz CTCSS
        "altitude_km": 520,
        "norad_id": 40931,
    },
    "PO-101": {
        "name": "PO-101",
        "rx_freq_hz": 145_900_000,  # Downlink (VHF)
        "tx_freq_hz": 437_500_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 1413,  # 141.3 Hz CTCSS
        "altitude_km": 400,
        "norad_id": 43678,
    },
    "RS95S": {
        "name": "RS95S",
        "rx_freq_hz": 436_950_000,
        "tx_freq_hz": 145_920_000,
        "rx_tone_hz": 0,
        "tx_tone_hz": 670,  # 67.0 Hz CTCSS
        "altitude_km": 510,
        "norad_id": 67291,
    },
    "TEVEL2": {
        "name": "TEVEL2",
        "rx_freq_hz": 436_400_000,  # Downlink (beacon band)
        "tx_freq_hz": 145_970_000,  # Uplink
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 500,
        "norad_id": 0,  # Multiple satellites (TEVEL2-1 thru 9)
    },
    # SSTV satellites (RX-only — no uplink)
    "ISS SSTV": {
        "name": "ISS SSTV",
        "rx_freq_hz": 145_800_000,  # SSTV downlink (PD120)
        "tx_freq_hz": 0,  # RX only
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 420,
        "norad_id": 25544,  # Same NORAD as ISS
    },
    "RS40S": {
        "name": "RS40S",
        "rx_freq_hz": 437_625_000,  # SSTV Robot 36 (UmKA-1)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 57172,
    },
    "RS38S": {
        "name": "RS38S",
        "rx_freq_hz": 437_825_000,  # SSTV Robot 36 (VIZARD-meteo)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 57174,
    },
    "RS58S": {
        "name": "RS58S",
        "rx_freq_hz": 435_290_000,  # SSTV Robot 36 (Monitor-3)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 57178,
    },
    "RS27S": {
        "name": "RS27S",
        "rx_freq_hz": 436_125_000,  # SSTV Robot 36 (UTMN-2)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 478,
        "norad_id": 57203,
    },
    "RS57S": {
        "name": "RS57S",
        "rx_freq_hz": 436_080_000,  # SSTV Robot 36 (Monitor-4)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 58635,
    },
    "RS18S": {
        "name": "RS18S",
        "rx_freq_hz": 437_350_000,  # SSTV Robot 36 (SakhaCube-Cholbon)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 57176,
    },
    "RS83S": {
        "name": "RS83S",
        "rx_freq_hz": 436_320_000,  # SSTV & SSDV (Lobachevsky)
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 58642,
    },
    "SONATE-2": {
        "name": "SONATE-2",
        "rx_freq_hz": 145_880_000,  # SSTV Martin M1
        "tx_freq_hz": 0,
        "rx_tone_hz": 0,
        "tx_tone_hz": 0,
        "altitude_km": 510,
        "norad_id": 44420,
    },
}

DEFAULT_SATS = [
    "ISS",
    "SO-50",
    "AO-91",
    "AO-123",
    "CAS-3H",
    "IO-86",
    "PO-101",
    "RS95S",
    "ISS SSTV",
]

ALL_SAT_NAMES = list(KNOWN_SATELLITES.keys())


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            if crc & 0x8000:
                crc = (crc << 1) ^ 0x1021
            else:
                crc <<= 1
            crc &= 0xFFFF
    return crc


def frame_crc(cmd: int, addr: int, data: bytes) -> int:
    buf = struct.pack(">BHH", cmd, addr, len(data)) + data
    return crc16_xmodem(buf)


def encode_frame(cmd: int, addr: int, data: bytes) -> bytes:
    """Encode a complete CPS frame."""
    header = bytes([HEADER, cmd])
    addr_len = struct.pack(">HH", addr, len(data))
    crc = frame_crc(cmd, addr, data)
    crc_bytes = struct.pack(">H", crc)
    return header + addr_len + data + crc_bytes


class CpsSession:
    def __init__(self, port: str, baudrate: int = 115200):
        self.port = port
        self.baudrate = baudrate
        self._ser = None

    def __enter__(self):
        import serial

        self._ser = serial.Serial(self.port, self.baudrate, timeout=3)
        return self

    def __exit__(self, *args):
        if self._ser is not None:
            self._ser.close()

    def handshake(self) -> bool:
        self._ser.reset_input_buffer()
        self._ser.write(HANDSHAKE)
        self._ser.flush()

        start = time.monotonic()
        while time.monotonic() - start < 5.0:
            if self._ser.in_waiting:
                byte = self._ser.read(1)[0]
                if byte == ACK:
                    return True
        return False

    def read_satellite(self, idx: int) -> bytes | None:
        addr = SAT_CPS_BASE + idx * SAT_RECORD_SIZE
        frame = encode_frame(CMD_READ, addr, b"")
        self._ser.write(frame)
        self._ser.flush()

        hdr = self._ser.read(8)
        if len(hdr) < 8:
            return None
        if hdr[0] != HEADER:
            return None

        cmd = hdr[1]
        if cmd == 0xEE:  # Error
            return None

        data_len = struct.unpack(">H", hdr[4:6])[0]
        data = self._ser.read(data_len + 2)  # data + crc
        if len(data) < data_len + 2:
            return None

        return data[:data_len]

    def write_satellite(self, idx: int, record: bytes) -> None:
        assert len(record) == SAT_RECORD_SIZE
        addr = SAT_CPS_BASE + idx * SAT_RECORD_SIZE
        frame = encode_frame(CMD_WRITE, addr, record)
        self._ser.write(frame)
        self._ser.flush()
        time.sleep(0.01)  # small gap between writes

    def end_session(self) -> None:
        """Send END command; radio will reset."""
        frame = encode_frame(CMD_END, 0, b"")
        self._ser.write(frame)
        self._ser.flush()
        time.sleep(0.5)


_GM = 3.986004418e14
_EARTH_R = 6_371_000


def altitude_from_mean_motion(n_rev_per_day: float) -> int:
    """Compute mean orbital altitude (km) from TLE mean motion.
    Kepler's third law
    Returns integer km.
    """
    if n_rev_per_day <= 0:
        return 0
    T = 86400.0 / n_rev_per_day
    a_m = (_GM * T * T / (4.0 * math.pi * math.pi)) ** (1.0 / 3.0)
    return max(0, int((a_m - _EARTH_R) / 1000.0 + 0.5))


def parse_mean_motion(line2: str) -> float | None:
    """Extract Mean Motion (rev/day) from TLE line 2.

    TLE format line 2, columns 53-63: Mean Motion, e.g. "15.50000000"
    """
    if len(line2) < 63:
        return None
    try:
        return float(line2[52:63])
    except ValueError:
        return None


def fetch_tles(
    url: str = "https://celestrak.org/NORAD/elements/gp.php?GROUP=amateur&FORMAT=tle",
) -> list[tuple[str, str, str]]:
    print(f"Fetching TLE data from {url} ...")
    try:
        with urlopen(url, timeout=30) as resp:
            text = resp.read().decode("utf-8", errors="replace")
    except OSError as e:
        print(f"Warning: failed to fetch TLE: {e}")
        return []

    lines = [l.rstrip("\r\n") for l in text.splitlines()]
    sats = []
    i = 0
    while i < len(lines):
        if not lines[i].strip():
            i += 1
            continue
        name = lines[i].strip()
        i += 1
        if i + 1 >= len(lines):
            break
        l1 = lines[i].strip()
        l2 = lines[i + 1].strip()
        i += 2

        if not l1.startswith("1 ") or not l2.startswith("2 "):
            continue
        if len(l1) < 69 or len(l2) < 69:
            continue

        sats.append((name, l1, l2))

    print(f"  Got {len(sats)} TLE entries")
    return sats


def parse_tle_file(path: str) -> list[tuple[str, str, str]]:
    with open(path) as f:
        lines = [l.rstrip("\r\n") for l in f.readlines()]

    sats = []
    i = 0
    while i < len(lines):
        if not lines[i].strip():
            i += 1
            continue
        name = lines[i].strip()
        i += 1
        if i + 1 >= len(lines):
            break
        l1 = lines[i].strip()
        l2 = lines[i + 1].strip()
        i += 2
        if not l1.startswith("1 ") or not l2.startswith("2 "):
            continue
        if len(l1) < 69 or len(l2) < 69:
            continue
        sats.append((name, l1, l2))
    return sats


def match_tle_to_sat(tle_name: str) -> str | None:
    tle_upper = tle_name.upper()

    name_map = {
        # FM repeaters
        "ISS (ZARYA)": "ISS",
        "ZARYA": "ISS",
        "SAUDISAT 1C": "SO-50",
        "RADFXSAT": "AO-91",
        "FOX-1B": "AO-91",
        "ASRTU-1": "AO-123",
        "LILACSAT-2": "CAS-3H",
        "LAPAN-A2": "IO-86",
        "LAPAN A2": "IO-86",
        "DIWATA-2": "PO-101",
        "DIWATA-2B": "PO-101",
        "QMR-KWT": "RS95S",
        "TEVEL2": "TEVEL2",
        "EYESAT A": "AO-27",
        # SSTV satellites
        "UMKA-1": "RS40S",
        "UMKA 1": "RS40S",
        "VIZARD-METEO": "RS38S",
        "VIZARD": "RS38S",
        "MONITOR-3": "RS58S",
        "MONITOR 3": "RS58S",
        "UTMN-2": "RS27S",
        "UTMN 2": "RS27S",
        "MONITOR-4": "RS57S",
        "MONITOR 4": "RS57S",
        "SAKHACUBE": "RS18S",
        "CHOLBON": "RS18S",
        "LOBACHEVSKY": "RS83S",
        "SONATE-2": "SONATE-2",
        "SONATE 2": "SONATE-2",
    }

    for pattern, key in name_map.items():
        if pattern in tle_upper:
            return key

    for key, info in KNOWN_SATELLITES.items():
        key_upper = key.upper()
        if key_upper == tle_upper:
            return key
        if f"({key_upper})" in tle_upper:
            return key

    return None


def build_satellites(
    selected: list[str],
    tle_entries: list[tuple[str, str, str]] | None = None,
) -> list[dict]:
    tle_altitudes: dict[str, int] = {}
    if tle_entries:
        for name, l1, l2 in tle_entries:
            matched = match_tle_to_sat(name)
            if matched and matched not in tle_altitudes:
                mm = parse_mean_motion(l2)
                if mm is not None:
                    alt = altitude_from_mean_motion(mm)
                    tle_altitudes[matched] = alt

        # Propagate ISS altitude to ISS SSTV (same NORAD 25544)
        if "ISS" in tle_altitudes:
            tle_altitudes.setdefault("ISS SSTV", tle_altitudes["ISS"])

    results = []
    for sat_name in selected:
        info = KNOWN_SATELLITES[sat_name].copy()

        if sat_name in tle_altitudes:
            tle_alt = tle_altitudes[sat_name]
            print(
                f"  {sat_name}: TLE altitude = {tle_alt} km "
                f"(was {info['altitude_km']} km default)"
            )
            info["altitude_km"] = tle_alt
        else:
            print(f"  {sat_name}: using default altitude = {info['altitude_km']} km")

        results.append(info)

    return results


def encode_sat_record(info: dict) -> bytes:
    buf = bytearray(SAT_RECORD_SIZE)

    # name: up to 10 ASCII bytes, null-padded
    name = info["name"].encode("ascii", errors="replace")[:10]
    buf[0 : len(name)] = name

    # frequencies: u32 LE in Hz
    struct.pack_into("<I", buf, 10, info["rx_freq_hz"])
    struct.pack_into("<I", buf, 14, info["tx_freq_hz"])

    # tones: u16 LE in tenths of Hz
    struct.pack_into("<H", buf, 18, info["rx_tone_hz"])
    struct.pack_into("<H", buf, 20, info["tx_tone_hz"])

    # altitude: u16 LE in km
    struct.pack_into("<H", buf, 22, info["altitude_km"])

    # flags: 0 (reserved)
    buf[24] = 0

    # remaining 7 bytes already zero-initialized
    return bytes(buf)


BLANK_RECORD = bytes([0xFF] * SAT_RECORD_SIZE)


def main():
    parser = argparse.ArgumentParser(
        description="Update satellite records on UV-K6 (aura firmware)"
    )
    parser.add_argument(
        "port",
        nargs="?",
        default=None,
        help="Serial port (e.g. /dev/ttyUSB0, COM3). Not needed for --list.",
    )
    parser.add_argument(
        "--sat",
        nargs="+",
        metavar="NAME",
        help="Satellite names to write (default: ISS SO-50 AO-91 AO-123). "
        "Use --list to see all known satellites.",
    )
    parser.add_argument(
        "--tle",
        action="store_true",
        help="Fetch live TLE data from celestrak to get accurate altitudes.",
    )
    parser.add_argument(
        "--tle-file",
        type=str,
        metavar="PATH",
        help="Read TLE data from a local file instead of fetching.",
    )
    parser.add_argument(
        "--baudrate",
        type=int,
        default=115200,
        help="Serial baud rate (default: 115200)",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List all built-in satellites with their parameters and exit.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be written without connecting to the radio.",
    )
    parser.add_argument(
        "--all-known",
        action="store_true",
        help="Write ALL known satellites (instead of just the defaults).",
    )
    parser.add_argument(
        "--rx-only",
        action="store_true",
        help="Clear TX frequencies (receive-only mode for all satellites).",
    )
    args = parser.parse_args()

    if args.list:
        print(
            f"{'Name':<10} {'Downlink':>12} {'Uplink':>12} {'RX Tone':>8} "
            f"{'TX Tone':>8} {'Alt(km)':>8}"
        )
        print("-" * 68)
        for name in ALL_SAT_NAMES:
            s = KNOWN_SATELLITES[name]
            rx = f"{s['rx_freq_hz'] / 1e6:.3f}M"
            tx = f"{s['tx_freq_hz'] / 1e6:.3f}M" if s["tx_freq_hz"] else "none"
            rxt = f"{s['rx_tone_hz'] / 10:.1f}" if s["rx_tone_hz"] else "none"
            txt = f"{s['tx_tone_hz'] / 10:.1f}" if s["tx_tone_hz"] else "none"
            print(
                f"{name:<10} {rx:>12} {tx:>12} {rxt:>8} {txt:>8} {s['altitude_km']:>8}"
            )
        return

    if not args.port:
        parser.error("Serial port is required (except for --list)")

    if args.sat:
        selected = []
        for name in args.sat:
            if name in KNOWN_SATELLITES:
                selected.append(name)
            else:
                print(
                    f"Warning: unknown satellite '{name}', skipping. "
                    f"Known: {', '.join(ALL_SAT_NAMES)}"
                )
        if not selected:
            print("Error: no valid satellite names specified.")
            sys.exit(1)
    elif args.all_known:
        selected = list(ALL_SAT_NAMES)
    else:
        selected = list(DEFAULT_SATS)

    if len(selected) > MAX_SATELLITES:
        print(
            f"Warning: {len(selected)} satellites exceeds max "
            f"({MAX_SATELLITES}), truncating."
        )
        selected = selected[:MAX_SATELLITES]

    tle_entries = None
    if args.tle:
        tle_entries = fetch_tles()
    elif args.tle_file:
        print(f"Reading TLE data from {args.tle_file} ...")
        tle_entries = parse_tle_file(args.tle_file)
        print(f"  Got {len(tle_entries)} TLE entries")

    print(f"\nSelected satellites ({len(selected)}):")
    satellites = build_satellites(selected, tle_entries)

    if args.rx_only:
        for s in satellites:
            s["tx_freq_hz"] = 0
            s["tx_tone_hz"] = 0
        print("  (TX frequencies cleared: receive-only mode)")

    # Encode
    records = []
    for i in range(MAX_SATELLITES):
        if i < len(satellites):
            records.append(encode_sat_record(satellites[i]))
        else:
            records.append(BLANK_RECORD)

    print(
        f"\nTotal slots: {len(records)} "
        f"({len([r for r in records if r != BLANK_RECORD])} filled, "
        f"{len([r for r in records if r == BLANK_RECORD])} blank)"
    )

    if args.dry_run:
        print("\n[Dry run — not writing to radio]")
        for i, rec in enumerate(records):
            if rec != BLANK_RECORD:
                # Decode for display
                name = rec[0:10].rstrip(b"\x00").decode("ascii", errors="replace")
                rx = struct.unpack_from("<I", rec, 10)[0]
                tx = struct.unpack_from("<I", rec, 14)[0]
                alt = struct.unpack_from("<H", rec, 22)[0]
                tx_str = f"TX={tx/1e6:.3f}M" if tx else "RX-only"
                print(f"  [{i:2d}] {name:<10} RX={rx/1e6:.3f}M {tx_str} alt={alt}km")
        return

    print(f"\nOpening {args.port} at {args.baudrate} baud...")
    try:
        import serial
    except ImportError:
        print("Error: pyserial is required. Install with: pip install pyserial")
        sys.exit(1)

    try:
        with CpsSession(args.port, args.baudrate) as sess:
            # Handshake
            print("Sending handshake... ", end="", flush=True)
            if not sess.handshake():
                print("FAILED")
                print(
                    "No ACK received. Make sure:\n"
                    "  - The radio is running aura firmware (not bootloader)\n"
                    "  - The serial cable is connected\n"
                    "  - The correct port is selected"
                )
                sys.exit(1)
            print("OK")

            # Write all 20 slots (first write triggers sector erase)
            print("Writing satellite records...")
            for i, rec in enumerate(records):
                name = (
                    rec[0:10]
                    .rstrip(b"\x00")
                    .rstrip(b"\xff")
                    .decode("ascii", errors="replace")
                )
                label = name if rec != BLANK_RECORD else "(blank)"
                print(f"  [{i:2d}] {label:<10} ... ", end="", flush=True)
                sess.write_satellite(i, rec)
                print("OK")

            # End session (radio resets)
            print("Ending CPS session (radio will reset)...")
            sess.end_session()

    except serial.SerialException as e:
        print(f"Error: cannot open serial port: {e}")
        sys.exit(1)

    print(
        f"\nDone! Wrote {len(satellites)} satellite(s) "
        f"({MAX_SATELLITES - len(satellites)} empty slots)"
    )


if __name__ == "__main__":
    main()
