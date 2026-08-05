mod chebyshev;
mod coeffs;

use crate::device::radio::Band;
use chebyshev::clenshaw_q16;
use coeffs::{dur2df_coeffs, select_cheb_coeffs};

/// State for an active Doppler pass, initialised once at tracking start.
pub struct DopplerState {
    /// Pointer to the selected Chebyshev-7 coefficient table (static).
    cheb: &'static [i32; 8],
    /// Estimated maximum Doppler shift in integer Hz for the RX frequency.
    /// Multiplied by the Q1.16 normalised value in `doppler_update()` to get
    /// the instantaneous Doppler shift.
    df_max_q: i32,
    /// Absolute AOS time (seconds since epoch, consistent with wall clock).
    pub(crate) aos_abs: u32,
    /// Pass duration in seconds.
    pub(crate) dur_sec: u32,
}

/// Estimate df_max (Hz) from pass duration and satellite altitude.
/// All values in Q1.16; returns Hz (integer, not Q1.16).
fn estimate_df_max(dur_sec: u32, altitude_km: u16, band: Band) -> i32 {
    let coeffs = dur2df_coeffs(band);
    let d_q = (dur_sec as i32) << 16; // Q1.16
    let h_q = (altitude_km as i32) << 16; // Q1.16

    // c0 * d^2  (Q16 * Q16 * Q16 -> shift 16 -> Q16)
    let t0 = ((coeffs[0] as i64 * d_q as i64 * d_q as i64) >> 32) as i32;

    // c1 * d  (Q16 * Q16 -> shift 16 -> Q16)
    let t1 = ((coeffs[1] as i64 * d_q as i64) >> 16) as i32;

    // c2 * d * h
    let t2 = ((coeffs[2] as i64 * d_q as i64 * h_q as i64) >> 32) as i32;

    // c3 * h
    let t3 = ((coeffs[3] as i64 * h_q as i64) >> 16) as i32;

    // c4 (already Q16)
    let t4 = coeffs[4];

    // Sum -> Q16 -> convert to Hz
    let df_max_q = t0
        .wrapping_add(t1)
        .wrapping_add(t2)
        .wrapping_add(t3)
        .wrapping_add(t4);
    // Convert Q1.16 to integer Hz: arithmetic right shift
    df_max_q >> 16
}

/// Initialise Doppler state for a satellite pass.
///
/// Call once when the user starts tracking.
///
/// - `aos_abs`: absolute AOS time (wall clock seconds, cross-midnight resolved)
/// - `los_abs`: absolute LOS time
/// - `max_el_deg`: maximum elevation angle in degrees
/// - `altitude_km`: satellite orbital altitude in km
/// - `band`: VHF or UHF (auto-detected from RX frequency)
pub fn doppler_init(
    aos_abs: u32,
    los_abs: u32,
    max_el_deg: u16,
    altitude_km: u16,
    band: Band,
) -> DopplerState {
    let dur_sec = los_abs.saturating_sub(aos_abs);
    let df_max = estimate_df_max(dur_sec, altitude_km, band);
    let cheb = select_cheb_coeffs(band, max_el_deg);

    DopplerState {
        cheb,
        // df_max is integer Hz, NOT Q1.16!
        df_max_q: df_max,
        aos_abs,
        dur_sec,
    }
}

/// Update Doppler correction for the current wall-clock time.
///
/// Call every second during tracking. Returns the Doppler shift in **Hz**
/// (positive = approaching, negative = receding). Returns 0 if outside the
/// pass window.
///
/// 1. t_norm = (now - aos) / dur -> map to Q1.16
/// 2. x = 2*t - 1   ->  [-1, 1]
/// 3. norm = clenshaw_q16(cheb, x)
/// 4. return norm * df_max (Hz)
pub fn doppler_update(state: &DopplerState, now_abs: u32) -> i32 {
    let elapsed = now_abs.saturating_sub(state.aos_abs) as i32;
    if elapsed <= 0 || elapsed as u32 >= state.dur_sec {
        return 0;
    }

    // t_q = elapsed / dur  in Q1.16
    let t_q = ((elapsed as i64) << 16) / state.dur_sec as i64;

    // x_q = 2*t_q - 1<<16 ->  map [0, 1] to [-1, 1]
    let x_q = (t_q as i32) * 2 - (1 << 16);

    // Clenshaw evaluation -> normalised value in Q1.16, range [-1, 1]
    let norm_q = clenshaw_q16(state.cheb, x_q);

    // norm_q * df_max_q -> Q32 -> shift 16 -> Hz
    ((norm_q as i64 * state.df_max_q as i64) >> 16) as i32
}
