use crate::device::radio::Band;

/// Chebyshev-7 coefficients in Q1.16 for the 2m band, 3 elevation bins.
/// Index order: [c0, c1, c2, c3, c4, c5, c6, c7].
///
/// Low elevation (5–26), CV avg max error 215 Hz.
pub const CHEB7_2M_LOW: [i32; 8] = [-869, -70013, 376, 5967, -58, -944, 10, 216];

/// Mid elevation (26–57), CV avg max error 328 Hz.
pub const CHEB7_2M_MID: [i32; 8] = [-204, -77140, 280, 15652, -88, -4802, 24, 2187];

/// High elevation (57–90), CV avg max error 406 Hz.
pub const CHEB7_2M_HIGH: [i32; 8] = [-224, -79212, 467, 19698, -226, -7196, 159, 4131];

/// Chebyshev-7 coefficients in Q1.16 for the 70cm band, 3 elevation bins.
///
/// Low elevation (5–22), CV avg max error 569 Hz.
pub const CHEB7_70CM_LOW: [i32; 8] = [15, -69374, -61, 5090, 22, -797, -7, 178];

/// Mid elevation (22–55), CV avg max error 752 Hz.
pub const CHEB7_70CM_MID: [i32; 8] = [-34, -77378, 17, 16074, -6, -5089, 5, 2407];

/// High elevation (55–90), CV avg max error 820 Hz.
pub const CHEB7_70CM_HIGH: [i32; 8] = [-7, -79773, 16, 20900, -10, -8055, 11, 5000];

/// dur -> df_max estimation coefficients in Q1.16 for the 2m band.
///
/// df_max = c0*dur^2 + c1*dur + c2*dur*alt + c3*alt + c4
/// where dur = pass duration in seconds, alt = satellite altitude in km.
pub const DUR2DF_2M: [i32; 5] = [-156, 461355, -27, -140350, 81041129];

/// dur->df_max estimation coefficients in Q1.16 for the 70cm band.
pub const DUR2DF_70CM: [i32; 5] = [75, 2339053, -2346, 120613, -64101833];

/// Elevation threshold for the low/mid boundary, in degrees * 256 (Q8.8).
/// 2m: <= 26.1
pub const ELEV_THRESH_LOW_2M: u16 = 26 * 256 + 26; // 26.1 degree

/// Elevation threshold for the mid/high boundary, in degrees * 256 (Q8.8).
/// 2m: <= 56.7
pub const ELEV_THRESH_HIGH_2M: u16 = 56 * 256 + 179; // 56.7°

/// 70cm: low <= 22.1°
pub const ELEV_THRESH_LOW_70CM: u16 = 22 * 256 + 26; // 22.1°

/// 70cm: high > 54.7°
pub const ELEV_THRESH_HIGH_70CM: u16 = 54 * 256 + 179; // 54.7°

/// Select the appropriate Chebyshev-7 coefficient array for a given band and
/// maximum elevation angle.
pub fn select_cheb_coeffs(band: Band, max_el_deg: u16) -> &'static [i32; 8] {
    let max_el_q8 = max_el_deg << 8;
    match band {
        Band::Vhf => {
            if max_el_q8 <= ELEV_THRESH_LOW_2M {
                &CHEB7_2M_LOW
            } else if max_el_q8 <= ELEV_THRESH_HIGH_2M {
                &CHEB7_2M_MID
            } else {
                &CHEB7_2M_HIGH
            }
        }
        Band::Uhf => {
            if max_el_q8 <= ELEV_THRESH_LOW_70CM {
                &CHEB7_70CM_LOW
            } else if max_el_q8 <= ELEV_THRESH_HIGH_70CM {
                &CHEB7_70CM_MID
            } else {
                &CHEB7_70CM_HIGH
            }
        }
    }
}

/// Return the dur->df_max coefficient array for the given band.
pub fn dur2df_coeffs(band: Band) -> &'static [i32; 5] {
    match band {
        Band::Vhf => &DUR2DF_2M,
        Band::Uhf => &DUR2DF_70CM,
    }
}
