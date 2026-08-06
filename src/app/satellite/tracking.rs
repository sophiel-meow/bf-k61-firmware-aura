use crate::device::radio::{Power, Radio, SubAudio};
use cortex_m::peripheral::SYST;

/// Scale Doppler shift from RX frequency to TX frequency proportionally.
/// Returns Doppler correction in Hz for the TX side.
pub fn scale_doppler_to_tx(doppler_rx_hz: i32, rx_freq_hz: u32, tx_freq_hz: u32) -> i32 {
    if rx_freq_hz > 0 {
        (doppler_rx_hz as i64 * tx_freq_hz as i64 / rx_freq_hz as i64) as i32
    } else {
        doppler_rx_hz
    }
}

pub fn tracking_ptt_press(
    radio: &mut Radio,
    syst: &mut SYST,
    tx_freq_hz: u32,
    doppler_tx_hz: i32,
    subaudio_tx: SubAudio,
    wide: bool,
) -> bool {
    let corrected_tx = (tx_freq_hz as i32 + doppler_tx_hz) as u32;

    // radio.set_frequency(tx_freq_hz);
    radio.set_tx_frequency(corrected_tx);
    radio.set_subaudio_tx(subaudio_tx);
    radio.set_power(Power::High);
    radio.set_wide_bandwidth(wide);

    radio.enter_tx(syst)
}

pub fn tracking_ptt_release(
    radio: &mut Radio,
    syst: &mut SYST,
    rx_freq_hz: u32,
    doppler_rx_hz: i32,
    wide: bool,
) {
    let corrected_rx = (rx_freq_hz as i32 + doppler_rx_hz) as u32;

    // end_tx() calls enter_rx() internally, but enter_rx will use whatever
    // radio.cfg.freq_hz was when end_tx() was called.  Set it *before*
    // end_tx so the RX frequency is correct on exit.
    radio.set_frequency(corrected_rx);
    radio.set_wide_bandwidth(wide);
    radio.end_tx(syst);

    // radio.retune_rx(syst, corrected_rx, wide);
}

/// Apply Doppler correction to the RX frequency (called every second).
/// Actually programs the FD6818 PLL so the hardware listens on the
/// corrected frequency, not just the in-memory config.
pub fn tracking_update_rx(
    radio: &mut Radio,
    syst: &mut SYST,
    base_rx_hz: u32,
    doppler_hz: i32,
    wide: bool,
) {
    let corrected = (base_rx_hz as i32 + doppler_hz) as u32;
    radio.retune_rx(syst, corrected, wide);
}

/// Apply squelch level and monitor mode during tracking.
pub fn tracking_set_squelch(radio: &mut Radio, syst: &mut SYST, sql: u8, monitor: bool) {
    radio.set_sql_level(syst, sql);
    radio.set_monitor(monitor);
}

/// Read all four gain values from the radio frontend.
pub fn tracking_read_gains(radio: &mut Radio, syst: &mut SYST) -> (u8, u8, u8, u16) {
    radio.read_rf_gains(syst)
}

/// Adjust a specific gain value. `menu`: 1=LNAs, 2=LNA, 3=PGA, 4=IF.
pub fn tracking_adjust_gain(radio: &mut Radio, syst: &mut SYST, menu: u8, up: bool) {
    radio.adjust_rf_gain(syst, menu, up);
}
