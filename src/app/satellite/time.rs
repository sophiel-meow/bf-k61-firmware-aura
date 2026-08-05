/// Convert a wall-clock time (HH:MM:SS) to seconds since midnight.
pub fn wall_secs_from_hms(h: u8, m: u8, s: u8) -> u32 {
    h as u32 * 3600 + m as u32 * 60 + s as u32
}

/// Convert seconds since midnight to HH:MM:SS.
pub fn hms_from_wall_secs(secs: u32) -> (u8, u8, u8) {
    let s = (secs % 60) as u8;
    let m = ((secs / 60) % 60) as u8;
    let h = (secs / 3600) as u8;
    (h, m, s)
}

/// SECONDS_PER_DAY
pub const SECS_PER_DAY: u32 = 86400;

/// Increment wall clock, wrapping at 86400 (midnight).
pub fn wall_clock_tick(wall_secs: &mut u32) {
    *wall_secs += 1;
    if *wall_secs >= SECS_PER_DAY {
        *wall_secs = 0;
    }
}

/// Resolve AOS and LOS to absolute timeline accounting for cross-midnight.
///
/// The user enters AOS and LOS as wall-clock times (HH:MM:SS on a particular
/// day). Since passes can cross midnight, we need to determine which calendar
/// day each of AOS and LOS falls on.
///
/// 1. LOS is the anchor — if LOS < current wall time, it's tomorrow.
/// 2. AOS is determined relative to LOS:
///     - If `aos_wall < los_wall`: same day as LOS
///     - If `aos_wall >= los_wall`: day before LOS (cross-midnight)
///
/// Returns `(aos_abs, los_abs)` where absolute times are in seconds since some
/// consistent epoch (current midnight + offset).
pub fn resolve_pass_time(current_wall: u32, aos_wall: u32, los_wall: u32) -> (u32, u32) {
    // Step 1: LOS anchor — if LOS is earlier than current, it's tomorrow
    let los_abs = if los_wall >= current_wall {
        los_wall
    } else {
        los_wall + SECS_PER_DAY
    };

    let los_day = (los_abs / SECS_PER_DAY) * SECS_PER_DAY;

    // Step 2: AOS relative to LOS
    let aos_abs = if aos_wall < los_wall {
        // Same calendar day as LOS
        los_day + aos_wall
    } else {
        // Day before LOS (cross-midnight case)
        los_day - SECS_PER_DAY + aos_wall
    };

    (aos_abs, los_abs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_same_day() {
        // current=14:00, AOS=14:30, LOS=14:45 → both same day
        let current = wall_secs_from_hms(14, 0, 0);
        let aos = wall_secs_from_hms(14, 30, 0);
        let los = wall_secs_from_hms(14, 45, 0);
        let (aos_abs, los_abs) = resolve_pass_time(current, aos, los);
        assert_eq!(aos_abs, 14 * 3600 + 30 * 60); // same day
        assert_eq!(los_abs, 14 * 3600 + 45 * 60); // same day
    }

    #[test]
    fn test_cross_midnight() {
        // current=23:50, AOS=23:55, LOS=00:10
        // LOS is tomorrow (00:10), AOS is today
        let current = wall_secs_from_hms(23, 50, 0);
        let aos = wall_secs_from_hms(23, 55, 0);
        let los = wall_secs_from_hms(0, 10, 0);
        let (aos_abs, los_abs) = resolve_pass_time(current, aos, los);

        // LOS tomorrow: 86400 + 600 = 87000
        assert_eq!(los_abs, SECS_PER_DAY + 600);
        // AOS same day as LOS? Since aos_wall(23:55=86100) > los_wall(0:10=600),
        // it's the day before LOS → los_day(86400) - 86400 + 86100 = 86100
        assert_eq!(aos_abs, 23 * 3600 + 55 * 60);
    }

    #[test]
    fn test_both_after_midnight() {
        // current=00:05, AOS=00:10, LOS=00:25
        // LOS >= current, so same day
        let current = wall_secs_from_hms(0, 5, 0);
        let aos = wall_secs_from_hms(0, 10, 0);
        let los = wall_secs_from_hms(0, 25, 0);
        let (aos_abs, los_abs) = resolve_pass_time(current, aos, los);
        assert_eq!(aos_abs, 600);
        assert_eq!(los_abs, 25 * 60);
    }

    #[test]
    fn test_cross_midnight_long_pass() {
        // current=23:00, AOS=23:30, LOS=01:00
        // LOS < current, so LOS is tomorrow
        let current = wall_secs_from_hms(23, 0, 0);
        let aos = wall_secs_from_hms(23, 30, 0);
        let los = wall_secs_from_hms(1, 0, 0);
        let (aos_abs, los_abs) = resolve_pass_time(current, aos, los);

        // LOS tomorrow: 86400 + 3600 = 90000
        assert_eq!(los_abs, SECS_PER_DAY + 3600);
        // AOS same day (aos_wall < los_wall)
        assert_eq!(aos_abs, SECS_PER_DAY + 23 * 3600 + 30 * 60);
    }
}
