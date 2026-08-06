import logging
import struct

from chirp import bitwise, chirp_common, directory, errors, memmap

LOG = logging.getLogger(__name__)

HANDSHAKE = b"PROGRAMBF-K6AURA"
ACK = 0x06

FRAME_HEADER = 0xA6
CMD_READ = 0x52
CMD_WRITE = 0x57
CMD_END = 0x45
CMD_ERROR = 0xEE

ADDR_CHANNELS = 0x0000
CHANNEL_SIZE = 32
CHANNEL_COUNT = 1000

ADDR_VFO = 0x8000
VFO_SIZE = 64

ADDR_SETTINGS = 0x9000
SETTINGS_SIZE = 28

ADDR_BOOT_TEXT = 0x9100
BOOT_TEXT_SIZE = 34

ADDR_BOOT_TUNE = 0x9200
BOOT_TUNE_SIZE = 96
BOOT_TUNE_PAIRS = 48

ADDR_BATTERY_CAL = 0x9300
BATTERY_CAL_SIZE = 2

ADDR_APRS = 0x9400
APRS_SETTINGS_SIZE = 52

ADDR_FM = 0xC000
FM_CHANNEL_COUNT = 30
FM_SIZE = FM_CHANNEL_COUNT * 2

IMAGE_SIZE = 0xC100


def _crc16_xmodem(data):
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if (crc & 0x8000) else (crc << 1)
        crc &= 0xFFFF
    return crc


def _encode_frame(cmd, addr, data):
    body = struct.pack(">BHH", cmd, addr, len(data)) + bytes(data)
    crc = _crc16_xmodem(body)
    return bytes([FRAME_HEADER]) + body + struct.pack(">H", crc)


def _read_exact(pipe, n):
    buf = b""
    while len(buf) < n:
        chunk = pipe.read(n - len(buf))
        if not chunk:
            raise errors.RadioError("timed out waiting for a response from the radio")
        buf += chunk
    return buf


def _recv_frame(pipe):
    header = _read_exact(pipe, 6)
    if header[0] != FRAME_HEADER:
        raise errors.RadioError("bad frame header from radio")
    cmd, addr, length = struct.unpack(">BHH", header[1:])
    data = _read_exact(pipe, length)
    crc_rx = struct.unpack(">H", _read_exact(pipe, 2))[0]
    if crc_rx != _crc16_xmodem(header[1:] + data):
        raise errors.RadioError("CRC mismatch in response from radio")
    if cmd == CMD_ERROR:
        raise errors.RadioError("radio reported protocol error 0x%02x" % data[0])
    return cmd, addr, data


HANDSHAKE_TIMEOUT_S = 1.0
HANDSHAKE_ATTEMPTS = 5


def _handshake(radio):
    radio.pipe.timeout = HANDSHAKE_TIMEOUT_S
    for _attempt in range(HANDSHAKE_ATTEMPTS):
        radio.pipe.write(HANDSHAKE)
        ack = radio.pipe.read(1)
        if ack == bytes([ACK]):
            return
    raise errors.RadioError(
        "no handshake ACK from radio after %d attempts — is this an "
        "Aura-firmware radio, and is the cable/port correct?" % HANDSHAKE_ATTEMPTS
    )


_TOTAL_STEPS = (CHANNEL_COUNT * CHANNEL_SIZE) // CHANNEL_SIZE + 7


def _read_region(radio, addr, length, step, status):
    out = bytearray()
    for off in range(0, length, step):
        radio.pipe.write(_encode_frame(CMD_READ, addr + off, b""))
        _, _, data = _recv_frame(radio.pipe)
        if len(data) != min(step, length - off):
            raise errors.RadioError("short read from radio at 0x%04x" % (addr + off))
        out += data
        status.cur += 1
        radio.status_fn(status)
    return bytes(out)


def _write_region(radio, addr, payload, step, status):
    for off in range(0, len(payload), step):
        chunk = payload[off : off + step]
        radio.pipe.write(_encode_frame(CMD_WRITE, addr + off, chunk))
        _recv_frame(radio.pipe)
        status.cur += 1
        radio.status_fn(status)


def do_download(radio):
    _handshake(radio)
    image = bytearray(b"\xff" * IMAGE_SIZE)

    status = chirp_common.Status()
    status.msg = "Downloading from radio"
    status.max = _TOTAL_STEPS
    status.cur = 0

    channels = _read_region(
        radio, ADDR_CHANNELS, CHANNEL_COUNT * CHANNEL_SIZE, CHANNEL_SIZE, status
    )
    image[ADDR_CHANNELS : ADDR_CHANNELS + len(channels)] = channels

    vfo = _read_region(radio, ADDR_VFO, VFO_SIZE, VFO_SIZE, status)
    image[ADDR_VFO : ADDR_VFO + len(vfo)] = vfo

    settings = _read_region(radio, ADDR_SETTINGS, SETTINGS_SIZE, SETTINGS_SIZE, status)
    image[ADDR_SETTINGS : ADDR_SETTINGS + len(settings)] = settings

    boot_text = _read_region(
        radio, ADDR_BOOT_TEXT, BOOT_TEXT_SIZE, BOOT_TEXT_SIZE, status
    )
    image[ADDR_BOOT_TEXT : ADDR_BOOT_TEXT + len(boot_text)] = boot_text

    boot_tune = _read_region(
        radio, ADDR_BOOT_TUNE, BOOT_TUNE_SIZE, BOOT_TUNE_SIZE, status
    )
    image[ADDR_BOOT_TUNE : ADDR_BOOT_TUNE + len(boot_tune)] = boot_tune

    fm = _read_region(radio, ADDR_FM, FM_SIZE, FM_SIZE, status)
    image[ADDR_FM : ADDR_FM + len(fm)] = fm

    battery_cal = _read_region(
        radio, ADDR_BATTERY_CAL, BATTERY_CAL_SIZE, BATTERY_CAL_SIZE, status
    )
    image[ADDR_BATTERY_CAL : ADDR_BATTERY_CAL + len(battery_cal)] = battery_cal

    aprs = _read_region(
        radio, ADDR_APRS, APRS_SETTINGS_SIZE, APRS_SETTINGS_SIZE, status
    )
    image[ADDR_APRS : ADDR_APRS + len(aprs)] = aprs

    radio.pipe.write(_encode_frame(CMD_END, 0, b""))
    _recv_frame(radio.pipe)

    return memmap.MemoryMapBytes(bytes(image))


def do_upload(radio):
    _handshake(radio)
    image = radio.get_mmap()

    status = chirp_common.Status()
    status.msg = "Uploading to radio"
    status.max = _TOTAL_STEPS
    status.cur = 0

    channels = bytes(
        image[ADDR_CHANNELS : ADDR_CHANNELS + CHANNEL_COUNT * CHANNEL_SIZE]
    )
    _write_region(radio, ADDR_CHANNELS, channels, CHANNEL_SIZE, status)

    vfo = bytes(image[ADDR_VFO : ADDR_VFO + VFO_SIZE])
    _write_region(radio, ADDR_VFO, vfo, VFO_SIZE, status)

    settings = bytes(image[ADDR_SETTINGS : ADDR_SETTINGS + SETTINGS_SIZE])
    _write_region(radio, ADDR_SETTINGS, settings, SETTINGS_SIZE, status)

    boot_text = bytes(image[ADDR_BOOT_TEXT : ADDR_BOOT_TEXT + BOOT_TEXT_SIZE])
    _write_region(radio, ADDR_BOOT_TEXT, boot_text, BOOT_TEXT_SIZE, status)

    boot_tune = bytes(image[ADDR_BOOT_TUNE : ADDR_BOOT_TUNE + BOOT_TUNE_SIZE])
    _write_region(radio, ADDR_BOOT_TUNE, boot_tune, BOOT_TUNE_SIZE, status)

    fm = bytes(image[ADDR_FM : ADDR_FM + FM_SIZE])
    _write_region(radio, ADDR_FM, fm, FM_SIZE, status)

    battery_cal = bytes(image[ADDR_BATTERY_CAL : ADDR_BATTERY_CAL + BATTERY_CAL_SIZE])
    _write_region(radio, ADDR_BATTERY_CAL, battery_cal, BATTERY_CAL_SIZE, status)

    aprs = bytes(image[ADDR_APRS : ADDR_APRS + APRS_SETTINGS_SIZE])
    _write_region(radio, ADDR_APRS, aprs, APRS_SETTINGS_SIZE, status)

    radio.pipe.write(_encode_frame(CMD_END, 0, b""))
    _recv_frame(radio.pipe)


MEM_FORMAT = """
struct channel {
  u8 rxfreq[4];
  u8 txfreq[4];
  ul16 rxtone;
  ul16 txtone;
  u8   dtmfgroup;
  u8   pttid;
  u8   power;
  u8   flags;
  ul32 decodercode;
  char name[12];
};

struct vfo {
  u8 freqdigits[8];
  ul16 rxtone;
  ul16 txtone;
  u16 remain0;
  u8 dtmfgroup;
  u8 ani;
  u8 power;
  u8 vfoflags;
  u8 remain1;
  u8 step;
  u8 offsetdigits[7];
  u8 spmute;
  ul32 decodercode;
};

struct settings {
  u8 sqllevel;
  u8 tailelim;
  u8 busylock;
  u8 txforbid;
  u8 keyautolock;
  u8 dualstandby;
  u8 voxswitch;
  u8 voxlevel;
  u8 totlevel;
  u8 beepsswitch;
  u8 rogerbeep;
  u8 scramblelevel;
  u8 contrast;
  u8 rtone;
  u8 scanmode;
  i8 ritoffset;
  u8 savelevel;
  u8 backlighttime;
  u8 chandispmode;
  // Programmable-key function assignments: each stores a
  // KEY_FUNCTIONS index 0..9
  u8 side1short;
  u8 side1long;
  u8 side2short;
  u8 side2long;
  u8 bandshort;
  u8 bandlong;
  u8 anitx;
  u8 rptrl;
  u8 voxdelay;
};

struct boottext {
  u8 bootdisplaymode;
  u8 bootsoundenabled;
  char bootline1[16];
  char bootline2[16];
};

struct boottune {
  u8 pairs[%(boot_tune_size)d];
};

struct batterycal {
  ul16 batterycalraw;
};

struct aprssettings {
  u8 bandlock;
  u8 unused0;
  u8 bkin;
  char aprscall[7];
  ul32 aprslat;
  ul32 aprslon;
  u8 aprspathidx;
  ul32 aprsfreq;
  u8 aprsdevinfo;
  char aprsdevname[7];
  u8 aprsbatvolt;
  char aprscomment[17];
  u8 aprsssid;
  u8 aprssymbolidx;
  u8 aprspower;
};

#seekto 0x0000;
struct channel channels[%(channel_count)d];

#seekto 0x8000;
struct vfo vfo_a;
struct vfo vfo_b;

#seekto 0x9000;
struct settings settings;

#seekto 0x9100;
struct boottext boottext;

#seekto 0x9200;
struct boottune boottune;

#seekto 0x9300;
struct batterycal batterycal;

#seekto 0x9400;
struct aprssettings aprssettings;
""" % {"channel_count": CHANNEL_COUNT, "boot_tune_size": BOOT_TUNE_SIZE}


def _deci_hz_to_hz(deci_hz):
    return deci_hz * 10


def _hz_to_deci_hz(hz):
    return hz // 10


def _bcd_byte_to_decimal(b):
    return (b >> 4) * 10 + (b & 0x0F)


def _decimal_to_bcd_byte(v):
    return ((v // 10) << 4) | (v % 10)


def _bcd4_to_deci_hz(raw4):
    return (
        _bcd_byte_to_decimal(raw4[3]) * 1_000_000
        + _bcd_byte_to_decimal(raw4[2]) * 10_000
        + _bcd_byte_to_decimal(raw4[1]) * 100
        + _bcd_byte_to_decimal(raw4[0])
    )


def _deci_hz_to_bcd4(deci_hz):
    return bytes(
        [
            _decimal_to_bcd_byte(deci_hz % 100),
            _decimal_to_bcd_byte((deci_hz // 100) % 100),
            _decimal_to_bcd_byte((deci_hz // 10_000) % 100),
            _decimal_to_bcd_byte(deci_hz // 1_000_000),
        ]
    )


def _u32_to_s32(v):
    v &= 0xFFFFFFFF
    return v - 0x1_0000_0000 if v & 0x8000_0000 else v


def _s32_to_u32(v):
    return v & 0xFFFFFFFF


def _decode_tone(raw):
    if raw == 0:
        return "", None, None
    if raw > 250:
        return "Tone", raw / 10.0, None
    inverted = raw > 105
    idx = (raw - 106) if inverted else (raw - 1)
    code = chirp_common.DTCS_CODES[idx]
    return "DTCS", code, "R" if inverted else "N"


BOOT_DISPLAY_MODES = ["None (skip)", "Voltage", "Message", "Logo"]

KEY_FUNCTIONS = [
    "None",
    "Wide/Narrow",
    "Monitor",
    "Mode",
    "TX Tone",
    "FM Radio",
    "Scan",
    "Power",
    "Flashlight",
    "Search",
    "Reverse",
    "APRS TX",
]

KEYFN_FIELDS = {
    "side1short",
    "side1long",
    "side2short",
    "side2long",
    "bandshort",
    "bandlong",
}

ROGER_TONE_MODES = ["Off", "Roger Beep", "MDC1200"]

BAND_LOCKS = [
    "CE/CN Ham (144-146 + 430-440 MHz)",
    "FCC Ham (144-148 + 420-450 MHz)",
    "UK Ham (144-148 + 430-440 MHz)",
    "400-430 MHz",
    "400-438 MHz",
    "PMR446",
    "GMRS/FRS/MURS",
    "CA Ham (144-148 + 430-450 MHz)",
    "All Locked (TX disabled)",
    "Unlocked (hardware range)",
]

BK_IN_MODES = ["Off", "Semi", "Full"]

APRS_PATHS = ["WIDE1-1,WIDE2-1", "WIDE2-2", "ARISS", "Direct (no digipeating)"]

APRS_SYMBOLS_LIST = [
    "Person",
    "Handheld",
    "Car",
    "Truck",
    "Bicycle",
    "House",
    "Weather Station",
    "Digipeater",
    "HF Gateway",
    "Balloon",
    "Aircraft",
]

APRS_POWER_LEVELS = ["Low", "Mid", "High"]

APRS_COORD_NOT_SET = 0x7FFF_FFFF


def _boot_tune_to_str(raw):
    pairs = []
    for i in range(BOOT_TUNE_PAIRS):
        tone, dur = raw[i * 2], raw[i * 2 + 1]
        if tone == 0 and dur == 0:
            break
        pairs.append(str(tone))
        pairs.append(str(dur))
    return ",".join(pairs)


def _str_to_boot_tune(text):
    values = [v.strip() for v in text.split(",") if v.strip() != ""]
    if len(values) % 2 != 0:
        raise errors.RadioError(
            "boot tune must be an even count of tone,duration values"
        )
    if len(values) // 2 > BOOT_TUNE_PAIRS:
        raise errors.RadioError(
            "boot tune supports at most %d note pairs" % BOOT_TUNE_PAIRS
        )
    out = bytearray(BOOT_TUNE_SIZE)
    for i in range(len(values) // 2):
        tone = int(values[i * 2])
        dur = int(values[i * 2 + 1])
        if not (0 <= tone <= 255 and 0 <= dur <= 255):
            raise errors.RadioError("boot tune values must be 0-255")
        out[i * 2] = tone
        out[i * 2 + 1] = dur
    return bytes(out)


def _encode_tone(mode, value, pol):
    if mode == "":
        return 0
    if mode == "Tone":
        return int(round(value * 10))
    if mode == "DTCS":
        idx = chirp_common.DTCS_CODES.index(value)
        return idx + 106 if pol == "R" else idx + 1
    raise errors.RadioError("unsupported tone mode %r" % mode)


@directory.register
class AuraBFK6Radio(chirp_common.CloneModeRadio):
    VENDOR = "Baofeng"
    MODEL = "UV-K6 (Aura)"
    BAUD_RATE = 115200
    NEEDS_COMPAT_SERIAL = False

    def sync_in(self):
        self._mmap = do_download(self)
        self.process_mmap()

    def sync_out(self):
        do_upload(self)

    def process_mmap(self):
        self._memobj = bitwise.parse(MEM_FORMAT, self._mmap)

    def get_features(self):
        rf = chirp_common.RadioFeatures()
        rf.has_bank = False
        rf.has_ctone = True
        rf.has_rx_dtcs = True
        rf.has_settings = True
        rf.valid_name_length = 12
        rf.valid_characters = chirp_common.CHARSET_ASCII
        rf.valid_duplexes = ["", "-", "+"]
        rf.valid_tmodes = ["", "Tone", "TSQL", "DTCS", "DTCS-R", "TSQL-R", "Cross"]
        rf.valid_cross_modes = [
            "Tone->Tone",
            "Tone->DTCS",
            "DTCS->Tone",
            "->Tone",
            "->DTCS",
            "DTCS->",
            "DTCS->DTCS",
        ]
        rf.valid_dtcs_codes = chirp_common.DTCS_CODES
        rf.valid_power_levels = [
            chirp_common.PowerLevel("High", watts=5.00),
            chirp_common.PowerLevel("Mid", watts=3.00),
            chirp_common.PowerLevel("Low", watts=1.00),
        ]
        rf.valid_tuning_steps = [2.5, 5.0, 6.25, 10.0, 12.5, 20.0, 25.0, 50.0, 100.0]
        rf.valid_bands = [(18000000, 620000000)]
        rf.valid_skips = [""]
        rf.memory_bounds = (0, CHANNEL_COUNT - 1)
        return rf

    def get_raw_memory(self, number):
        return repr(self._memobj.channels[number])

    def get_memory(self, number):
        _mem = self._memobj.channels[number]
        mem = chirp_common.Memory()
        mem.number = number

        if _mem.get_raw() == b"\xff" * CHANNEL_SIZE:
            mem.empty = True
            return mem

        rx_raw = bytes(int(b) for b in _mem.rxfreq)
        tx_raw = bytes(int(b) for b in _mem.txfreq)
        rxfreq_hz = _deci_hz_to_hz(_bcd4_to_deci_hz(rx_raw))
        if rxfreq_hz == 0:
            mem.empty = True
            return mem

        mem.freq = rxfreq_hz
        txfreq_hz = _deci_hz_to_hz(_bcd4_to_deci_hz(tx_raw))
        if txfreq_hz == rxfreq_hz or txfreq_hz == 0:
            mem.duplex = ""
        elif txfreq_hz > rxfreq_hz:
            mem.duplex = "+"
            mem.offset = txfreq_hz - rxfreq_hz
        else:
            mem.duplex = "-"
            mem.offset = rxfreq_hz - txfreq_hz

        mem.name = str(_mem.name).rstrip("\x00\xff ")
        mem.mode = "NFM" if (int(_mem.flags) & 0x40) else "FM"
        mem.power = [
            chirp_common.PowerLevel("High", watts=5.00),
            chirp_common.PowerLevel("Mid", watts=3.00),
            chirp_common.PowerLevel("Low", watts=1.00),
        ][min(int(_mem.power), 2)]

        rxmode, rxval, rxpol = _decode_tone(int(_mem.rxtone))
        txmode, txval, txpol = _decode_tone(int(_mem.txtone))
        chirp_common.split_tone_decode(
            mem, (txmode, txval, txpol), (rxmode, rxval, rxpol)
        )

        return mem

    def set_memory(self, mem):
        _mem = self._memobj.channels[mem.number]

        if mem.empty:
            _mem.set_raw(b"\xff" * CHANNEL_SIZE)
            return

        _mem.rxfreq.set_raw(_deci_hz_to_bcd4(_hz_to_deci_hz(mem.freq)))
        if mem.duplex == "+":
            tx_hz = mem.freq + mem.offset
        elif mem.duplex == "-":
            tx_hz = mem.freq - mem.offset
        else:
            tx_hz = mem.freq
        _mem.txfreq.set_raw(_deci_hz_to_bcd4(_hz_to_deci_hz(tx_hz)))

        name = mem.name.ljust(12, "\x00")[:12]
        _mem.name = name

        flags = int(_mem.flags) & ~0x40
        if mem.mode == "NFM":
            flags |= 0x40
        _mem.flags = flags

        _mem.power = {"High": 0, "Mid": 1, "Low": 2}.get(
            str(mem.power) if mem.power else "High", 0
        )

        (txmode, txval, txpol), (rxmode, rxval, rxpol) = chirp_common.split_tone_encode(
            mem
        )
        _mem.txtone = _encode_tone(txmode, txval, txpol)
        _mem.rxtone = _encode_tone(rxmode, rxval, rxpol)

    def get_settings(self):
        from chirp.settings import (
            RadioSetting,
            RadioSettingGroup,
            RadioSettings,
            RadioSettingValueInteger,
            RadioSettingValueBoolean,
            RadioSettingValueList,
            RadioSettingValueString,
        )

        s = self._memobj.settings
        top = RadioSettingGroup("aura", "Aura Settings")

        def bool_setting(field, name):
            top.append(
                RadioSetting(
                    field, name, RadioSettingValueBoolean(bool(int(getattr(s, field))))
                )
            )

        def int_setting(field, name, minv, maxv):
            top.append(
                RadioSetting(
                    field,
                    name,
                    RadioSettingValueInteger(minv, maxv, int(getattr(s, field))),
                )
            )

        def keyfn_setting(group, field, name):
            group.append(
                RadioSetting(
                    field,
                    name,
                    RadioSettingValueList(
                        KEY_FUNCTIONS,
                        KEY_FUNCTIONS[int(getattr(s, field)) % len(KEY_FUNCTIONS)],
                    ),
                )
            )

        int_setting("sqllevel", "Squelch level", 0, 9)
        bool_setting("tailelim", "Squelch tail elimination")
        bool_setting("busylock", "Busy channel lockout")
        bool_setting("txforbid", "TX inhibit")
        int_setting("keyautolock", "Keypad auto-lock (0=off, steps of 5s)", 0, 3)
        bool_setting("dualstandby", "Dual-watch standby")
        bool_setting("voxswitch", "VOX enable")
        int_setting("voxlevel", "VOX sensitivity", 1, 9)
        int_setting("totlevel", "TX timeout (steps of 15s, 0=off)", 0, 255)
        bool_setting("beepsswitch", "Keypress beep")
        top.append(
            RadioSetting(
                "rogerbeep",
                "Roger beep",
                RadioSettingValueList(
                    ROGER_TONE_MODES,
                    ROGER_TONE_MODES[int(s.rogerbeep) % len(ROGER_TONE_MODES)],
                ),
            )
        )
        int_setting("scramblelevel", "Voice scramble group (0=off)", 0, 3)
        int_setting("contrast", "LCD contrast", 0, 4)
        int_setting("rtone", "Repeater access tone", 0, 3)
        int_setting("scanmode", "Scan resume mode", 0, 2)
        int_setting(
            "ritoffset", "RIT / clarifier offset (10Hz steps, USB mode only)", -127, 127
        )
        int_setting("savelevel", "RX power-save level (0=off)", 0, 4)
        int_setting("backlighttime", "Backlight timeout (0=always on)", 0, 4)
        int_setting(
            "chandispmode",
            "Standby channel display (0=freq, 1=name, 2=name+freq)",
            0,
            2,
        )
        bool_setting("anitx", "Send ANI on PTT press")
        int_setting("rptrl", "RX mute after PTT release (steps of 100ms)", 0, 10)
        int_setting("voxdelay", "VOX hang time (steps of 0.1s, 0=0.5s)", 0, 15)

        keys = RadioSettingGroup("keys", "Programmable Keys")
        keyfn_setting(keys, "side1short", "Side1 short press")
        keyfn_setting(keys, "side1long", "Side1 long press")
        keyfn_setting(keys, "side2short", "Side2 short press")
        keyfn_setting(keys, "side2long", "Side2 long press")
        keyfn_setting(keys, "bandshort", "Band short press")
        keyfn_setting(keys, "bandlong", "Band long press")
        top.append(keys)

        boot = RadioSettingGroup("boot", "Boot Screen")
        bt = self._memobj.boottext

        boot.append(
            RadioSetting(
                "boottext.bootdisplaymode",
                "Boot display",
                RadioSettingValueList(
                    BOOT_DISPLAY_MODES, BOOT_DISPLAY_MODES[int(bt.bootdisplaymode) % 4]
                ),
            )
        )
        boot.append(
            RadioSetting(
                "boottext.bootsoundenabled",
                "Play boot tune",
                RadioSettingValueBoolean(bool(int(bt.bootsoundenabled))),
            )
        )
        boot.append(
            RadioSetting(
                "boottext.bootline1",
                "Boot text line 1",
                RadioSettingValueString(
                    0, 16, str(bt.bootline1).rstrip("\x00\xff"), autopad=False
                ),
            )
        )
        boot.append(
            RadioSetting(
                "boottext.bootline2",
                "Boot text line 2",
                RadioSettingValueString(
                    0, 16, str(bt.bootline2).rstrip("\x00\xff"), autopad=False
                ),
            )
        )

        tune_raw = bytes(int(b) for b in self._memobj.boottune.pairs)
        boot.append(
            RadioSetting(
                "boottune",
                "Boot tune (OpenGD77-style tone,duration,... pairs)",
                RadioSettingValueString(
                    0, 255, _boot_tune_to_str(tune_raw), autopad=False
                ),
            )
        )
        top.append(boot)

        battery = RadioSettingGroup("battery", "Battery Calibration")
        bc = self._memobj.batterycal
        battery.append(
            RadioSetting(
                "batterycal.batterycalraw",
                "Raw ADC calibration point (see on-radio BATCAL menu; "
                "raw ADC value, not converted to volts)",
                RadioSettingValueInteger(0, 65535, int(bc.batterycalraw)),
            )
        )
        top.append(battery)

        aprs = RadioSettingGroup("aprs", "Frequency Lock / Break-In / APRS")
        a = self._memobj.aprssettings

        aprs.append(
            RadioSetting(
                "aprssettings.bandlock",
                "TX frequency lock (FLOCK)",
                RadioSettingValueList(
                    BAND_LOCKS, BAND_LOCKS[int(a.bandlock) % len(BAND_LOCKS)]
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.bkin",
                "CW break-in",
                RadioSettingValueList(
                    BK_IN_MODES, BK_IN_MODES[int(a.bkin) % len(BK_IN_MODES)]
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprscall",
                "APRS callsign (no SSID)",
                RadioSettingValueString(
                    0, 6, str(a.aprscall).rstrip("\x00\xff"), autopad=False
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprsssid",
                "APRS SSID (0-15, e.g. 7 for CALL-7)",
                RadioSettingValueInteger(0, 15, int(a.aprsssid)),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprsfreq",
                "APRS beacon TX frequency (Hz)",
                RadioSettingValueInteger(0, 620_000_000, int(a.aprsfreq)),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprspathidx",
                "Digipeater path",
                RadioSettingValueList(
                    APRS_PATHS, APRS_PATHS[int(a.aprspathidx) % len(APRS_PATHS)]
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprssymbolidx",
                "APRS symbol",
                RadioSettingValueList(
                    APRS_SYMBOLS_LIST,
                    APRS_SYMBOLS_LIST[int(a.aprssymbolidx) % len(APRS_SYMBOLS_LIST)],
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprspower",
                "APRS beacon TX power",
                RadioSettingValueList(
                    APRS_POWER_LEVELS,
                    APRS_POWER_LEVELS[int(a.aprspower) % len(APRS_POWER_LEVELS)],
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprslat",
                "APRS latitude (degrees x 100000; negative = south; %d = not set)"
                % APRS_COORD_NOT_SET,
                RadioSettingValueInteger(
                    -90_00000, APRS_COORD_NOT_SET, _u32_to_s32(int(a.aprslat))
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprslon",
                "APRS longitude (degrees x 100000; negative = west; %d = not set)"
                % APRS_COORD_NOT_SET,
                RadioSettingValueInteger(
                    -180_00000, APRS_COORD_NOT_SET, _u32_to_s32(int(a.aprslon))
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprsdevinfo",
                "Include device name in beacon comment",
                RadioSettingValueBoolean(bool(int(a.aprsdevinfo))),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprsdevname",
                "Device name (shown in beacon comment)",
                RadioSettingValueString(
                    0, 6, str(a.aprsdevname).rstrip("\x00\xff"), autopad=False
                ),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprsbatvolt",
                "Include battery voltage in beacon comment",
                RadioSettingValueBoolean(bool(int(a.aprsbatvolt))),
            )
        )
        aprs.append(
            RadioSetting(
                "aprssettings.aprscomment",
                "Custom beacon comment text",
                RadioSettingValueString(
                    0, 16, str(a.aprscomment).rstrip("\x00\xff"), autopad=False
                ),
            )
        )
        top.append(aprs)

        return RadioSettings(top)

    def set_settings(self, settings):
        from chirp.settings import RadioSetting

        s = self._memobj.settings
        for element in settings:
            if not isinstance(element, RadioSetting):
                self.set_settings(element)
                continue

            name = element.get_name()
            if name == "boottext.bootdisplaymode":
                s2 = self._memobj.boottext
                s2.bootdisplaymode = BOOT_DISPLAY_MODES.index(str(element.value))
            elif name == "boottext.bootsoundenabled":
                self._memobj.boottext.bootsoundenabled = int(bool(element.value))
            elif name == "boottext.bootline1":
                self._memobj.boottext.bootline1 = str(element.value).ljust(16, "\x00")[
                    :16
                ]
            elif name == "boottext.bootline2":
                self._memobj.boottext.bootline2 = str(element.value).ljust(16, "\x00")[
                    :16
                ]
            elif name == "boottune":
                self._memobj.boottune.pairs = _str_to_boot_tune(str(element.value))
            elif name == "batterycal.batterycalraw":
                self._memobj.batterycal.batterycalraw = int(element.value)
            elif name == "rogerbeep":
                s.rogerbeep = ROGER_TONE_MODES.index(str(element.value))
            elif name == "aprssettings.bandlock":
                self._memobj.aprssettings.bandlock = BAND_LOCKS.index(
                    str(element.value)
                )
            elif name == "aprssettings.bkin":
                self._memobj.aprssettings.bkin = BK_IN_MODES.index(str(element.value))
            elif name == "aprssettings.aprscall":
                self._memobj.aprssettings.aprscall = str(element.value).ljust(
                    7, "\x00"
                )[:7]
            elif name == "aprssettings.aprsssid":
                self._memobj.aprssettings.aprsssid = int(element.value)
            elif name == "aprssettings.aprsfreq":
                self._memobj.aprssettings.aprsfreq = int(element.value)
            elif name == "aprssettings.aprspathidx":
                self._memobj.aprssettings.aprspathidx = APRS_PATHS.index(
                    str(element.value)
                )
            elif name == "aprssettings.aprssymbolidx":
                self._memobj.aprssettings.aprssymbolidx = APRS_SYMBOLS_LIST.index(
                    str(element.value)
                )
            elif name == "aprssettings.aprspower":
                self._memobj.aprssettings.aprspower = APRS_POWER_LEVELS.index(
                    str(element.value)
                )
            elif name == "aprssettings.aprslat":
                self._memobj.aprssettings.aprslat = _s32_to_u32(int(element.value))
            elif name == "aprssettings.aprslon":
                self._memobj.aprssettings.aprslon = _s32_to_u32(int(element.value))
            elif name == "aprssettings.aprsdevinfo":
                self._memobj.aprssettings.aprsdevinfo = int(bool(element.value))
            elif name == "aprssettings.aprsdevname":
                self._memobj.aprssettings.aprsdevname = str(element.value).ljust(
                    7, "\x00"
                )[:7]
            elif name == "aprssettings.aprsbatvolt":
                self._memobj.aprssettings.aprsbatvolt = int(bool(element.value))
            elif name == "aprssettings.aprscomment":
                self._memobj.aprssettings.aprscomment = str(element.value).ljust(
                    17, "\x00"
                )[:17]
            elif name in KEYFN_FIELDS:
                setattr(s, name, KEY_FUNCTIONS.index(str(element.value)))
            else:
                setattr(s, name, int(element.value))
