# Settings Menu Reference

From the standby screen, press **MENU** to open the app menu, then select **Settings**. Inside the settings menu:

- **UP/DOWN** move between items (or between the 7 category groups, at the top level).
- **MENU** opens the selected group, or starts editing the selected value.
- **UP/DOWN** while editing changes the value.
- **EXIT** backs out one level at a time.

The 7 groups, in the order they appear on the radio: **RADIO**, **AUDIO**, **DISP**, **KEYS**, **ANI**, **DIGI**, **SYSTEM**.

## RADIO

| On-screen | Meaning |
|---|---|
| `SQL` | Squelch level, 0–9. Higher = requires a stronger signal to open audio. |
| `STEP` | Frequency tuning step, from 2.5 kHz up to 10 kHz. |
| `TOT` | Transmit timeout timer. `0` = off; otherwise in 15-second steps (`1` = 15s … `12` = 180s) — TX cuts off automatically once this elapses. |
| `TDR` | Dual watch (dual standby): ON monitors both VFOs/channels, alternating between them while idle. |
| `SAVE` | Battery-save duty cycle in standby. `0` = off; `1`–`4` = increasingly aggressive power saving (longer radio-off intervals between RX checks — saves more battery, but slightly slower to notice an incoming signal). |
| `BCL` | Busy channel lockout: ON blocks transmitting while the channel is busy (a signal is present). |
| `TXINH` | Transmit inhibit: ON disables transmitting entirely (receive-only). |
| `W/N` | Channel bandwidth: `WIDE` or `NARROW` FM. |
| `PWR` | Transmit power: `HIGH` / `MID` / `LOW`. |
| `R-CTC` | Receive subaudio squelch tone (CTCSS or DCS) |
| `T-CTC` | Transmit subaudio tone |
| `SHIFT` | Repeater shift direction relative to the receive frequency: `OFF` (simplex) / `+` / `−`. |
| `OFFSET` | Repeater shift amount in kHz, used when `SHIFT` is `+` or `−`. |
| `SCRM` | Voice scrambler (inversion) level: `OFF` or preset `1`–`3`. Both ends need the same setting to understand each other. |
| `SCANMD` | What stops a channel scan: `TIME` (pause briefly, then keep scanning), `CARR` (stop while a carrier is present), `STOP` (stop on the first signal found and stay). |
| `RIT` | Receive incremental tuning (clarifier) offset in Hz, ±1270 Hz in 10 Hz steps. Mainly relevant in USB mode. |
| `CHDISP` | Channel display: `FREQ` (frequency), `NAME` (channel name), or `NAME+F` (name and frequency). |
| `BK-IN` | CW break-in: `OFF`, `SEMI` (auto-switch to RX between keying), or `FULL`. Only adjustable while in a CW mode. |

## AUDIO

| On-screen | Meaning |
|---|---|
| `BEEP` | Key-press confirmation beep, ON/OFF. |
| `VOX` | Voice-operated transmit: ON lets your voice trigger PTT hands-free. |
| `VOXLV` | VOX sensitivity, `1`–`9` (higher = triggers on quieter audio). |
| `VOXDLY` | How long VOX keeps transmitting after you stop talking, `0.5S`–`2.0S`. |
| `RTONE` | 1750 Hz-style repeater access tone frequency (4 presets) — sent by holding whichever key you've assigned to `TX TONE` (see `KEYS`). |
| `STE` | Squelch tail elimination: ON mutes the brief noise burst when a transmission ends. |
| `RPTRL` | Repeater/hang delay: how long the radio waits after you release PTT before actually dropping back to receive. `OFF` = instant; otherwise in 100 ms steps. |
| `ROGER` | End-of-transmission tone sent when you release PTT: `OFF`, `ROGER` beep, or `MDC1200` burst. |

## DISP

| On-screen | Meaning |
|---|---|
| `ABR` | Backlight auto-off timer: `OFF` (always on) or `5S`/`10S`/`15S`/`20S` of inactivity. |
| `AUTOLK` | Keypad auto-lock timer: `OFF` or locks after `5S`/`10S`/`15S` of inactivity. |
| `CONTR` | LCD contrast level, `0`–`4`. |

## KEYS

Assigns a function to each programmable key press. All six entries share the same list of functions: `NONE`, `WIDE/NAR`, `MONITOR`, `MODE`, `TX TONE`, `FM RADIO`, `SCAN`, `POWER`, `LIGHT`, `SEARCH`, `REVERSE`, `APRS TX`.

| On-screen | Meaning |
|---|---|
| `S1-SH` | Side Key 1, short press. |
| `S1-LG` | Side Key 1, long press. |
| `S2-SH` | Side Key 2, short press. |
| `S2-LG` | Side Key 2, long press. |
| `BND-SH` | BAND key (Search Key), short press. |
| `BND-LG` | BAND key (Search Key), long press. |

## ANI

Automatic DTMF ID/paging sent along with your transmission.

| On-screen | Meaning |
|---|---|
| `ANI-TX` | ON automatically sends a DTMF ID at the start of every transmission. |
| `CALL` | Which stored contact's DTMF code to send (selects from the contact list). `----` (none) disables it even if `ANI-TX` is on. |

## DIGI (SSTV, APRS)

Settings for the SSTV & APRS (AX.25 AFSK)

| On-screen | Meaning |
|---|---|
| `CALLSIGN` | Callsign, used as the APRS source address & SSTV text overlay |
| `SSID` | APRS SSID suffix, `-0` to `-15` (e.g. `-7` for a handheld). |
| `FREQ` | Transmit frequency for the APRS beacon. |
| `LAT` / `LON` | Fixed station latitude/longitude sent in the beacon (this radio has no GPS, so position is set manually). `----` means unset — a beacon won't be sent unless both are set. |
| `PATH` | Digipeater path: `W1-1,W2-1` (default general-purpose path) / `WIDE2-2` / `ARISS` (for satellite/ISS work) / `DIRECT` (no digipeaters). |
| `NAME-EN` | ON includes the device name (`DEVNAME`) at the start of the beacon comment. |
| `DEVNAME` | Custom device/model name included in the comment when `NAME-EN` is on. |
| `BATT-EN` | ON includes the current battery voltage in the beacon comment. |
| `COMMENT` | Free-text comment appended to the beacon. |
| `SYMBOL` | APRS map symbol shown for your station: `PERSON`, `HANDHELD`, `CAR`, `TRUCK`, `BICYCLE`, `HOUSE`, `WEATHER`, `DIGIPEATER`, `HF_GATEWAY`, `BALLOON`, or `AIRCRAFT`. |
| `POWER` | Transmit power used specifically for the APRS beacon: `LOW` / `MID` / `HIGH`. |

## SYSTEM

| On-screen | Meaning |
|---|---|
| `FLOCK` | TX frequency lock / band plan: region presets (`CE HAM`, `FCC HAM`, `GB HAM`, `CA HAM`), fixed ranges (`400-430`, `400-438`), license-free presets (`PMR446`, `GMRS/FRS`), `ALL LOCK` (TX disabled everywhere), or `UNLOCK` (full hardware range — see the warning at the top of the [README](../README.md)). |
| `BATCAL` | Battery voltage calibration: enter the voltage your multimeter reads across the battery to correct the on-screen reading. |
| `BOOTSCR` | What's shown on power-on: `NONE`, `VOLT` (battery voltage), `MSG` (two lines of custom text), or `LOGO` (a custom bitmap — see [Importing a Boot Logo](Boot-Logo.md)). |
| `BOOTSND` | ON plays a startup melody on power-on. Only selectable when `BOOTSCR` isn't `NONE`. |
| `VER` | Shows the firmware version |
| `RESET` | Factory reset. Press MENU again to confirm — this erases all settings, channels, and CPS-programmed data. |
