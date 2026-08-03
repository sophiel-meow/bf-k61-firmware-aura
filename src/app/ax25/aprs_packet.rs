use super::ax25::Ax25Builder;

pub const APRS_DEST_CALL: &[u8; 6] = b"APZK6X";

pub const WIDE1_1: &[u8; 6] = b"WIDE1 ";
pub const WIDE2_1: &[u8; 6] = b"WIDE2 ";

pub const WIDE2_2: &[u8; 6] = b"WIDE2 ";

pub const SYMBOL_HANDHELD: u8 = b'[';

const MAX_STATUS_LEN: usize = 67;

const MAX_POSITION_INFO_LEN: usize = 128;

pub fn format_coordinate(out: &mut [u8; 11], coord_i32: i32, lat: bool) -> &[u8] {
    let neg = coord_i32 < 0;
    let mut v = if neg { -coord_i32 } else { coord_i32 };

    // v is degrees * 100_000
    let deg = (v / 100_000) as u32;
    v -= deg as i32 * 100_000;
    let min = (v * 60 / 100_000) as u32; // whole minutes
    let min_frac = ((v * 60) % 100_000) / 1000; // hundredths of a minute

    let mut pos: usize = 0;

    // Degrees field: 2 digits for lat, 3 for lon
    if lat {
        // DDMM.hh
        out[pos] = b'0' + ((deg / 10) % 10) as u8;
        pos += 1;
        out[pos] = b'0' + (deg % 10) as u8;
        pos += 1;
    } else {
        // DDDMM.hh
        out[pos] = b'0' + ((deg / 100) % 10) as u8;
        pos += 1;
        out[pos] = b'0' + ((deg / 10) % 10) as u8;
        pos += 1;
        out[pos] = b'0' + (deg % 10) as u8;
        pos += 1;
    }

    // Minutes
    out[pos] = b'0' + ((min / 10) % 10) as u8;
    pos += 1;
    out[pos] = b'0' + (min % 10) as u8;
    pos += 1;

    // Decimal
    out[pos] = b'.';
    pos += 1;

    // Hundredths
    out[pos] = b'0' + ((min_frac / 10) % 10) as u8;
    pos += 1;
    out[pos] = b'0' + (min_frac % 10) as u8;
    pos += 1;

    // Hemisphere
    out[pos] = if lat {
        if neg {
            b'S'
        } else {
            b'N'
        }
    } else if neg {
        b'W'
    } else {
        b'E'
    };
    pos += 1;

    &out[..pos]
}

pub struct AprsConfig {
    pub src_call: [u8; 6],
    pub src_ssid: u8,
    pub dest_call: [u8; 6],
    pub dest_ssid: u8,
    pub digi_path: [([u8; 6], u8); 4],
    /// Number of valid entries in `digi_path`.
    pub digi_count: u8,
    /// Primary table symbol character.
    pub symbol_table: u8,
    /// Symbol character.
    pub symbol_code: u8,
    /// Latitude, degrees * 100_000 (negative = south).
    pub lat: i32,
    /// Longitude, degrees * 100_000 (negative = west).
    pub lon: i32,
    pub comment: [u8; 44],
    pub comment_len: u8,
}

impl AprsConfig {
    pub fn new(src_call: [u8; 6], src_ssid: u8) -> Self {
        AprsConfig {
            src_call,
            src_ssid,
            dest_call: *APRS_DEST_CALL,
            dest_ssid: 0,
            digi_path: [([b' '; 6], 0); 4],
            digi_count: 0,
            symbol_table: 0,
            symbol_code: 0,
            lat: 0,
            lon: 0,
            comment: [0; 44],
            comment_len: 0,
        }
    }

    pub fn build_position_report(&self, builder: &mut Ax25Builder) -> u16 {
        let mut info = [0u8; MAX_POSITION_INFO_LEN];
        let info_len = self.build_position_info(&mut info);
        builder.build_ui_frame(
            &self.dest_call,
            self.dest_ssid,
            &self.src_call,
            self.src_ssid,
            &self.digi_path[..self.digi_count as usize],
            &info[..info_len],
        )
    }

    pub fn build_position_info(&self, out: &mut [u8]) -> usize {
        let mut pos: usize = 0;

        out[pos] = b'!';
        pos += 1;

        // Latitude
        let mut lat_buf = [0u8; 11];
        let lat_str = format_coordinate(&mut lat_buf, self.lat, true);
        out[pos..pos + lat_str.len()].copy_from_slice(lat_str);
        pos += lat_str.len();

        // Symbol table
        out[pos] = self.symbol_table;
        pos += 1;

        // Longitude
        let mut lon_buf = [0u8; 11];
        let lon_str = format_coordinate(&mut lon_buf, self.lon, false);
        out[pos..pos + lon_str.len()].copy_from_slice(lon_str);
        pos += lon_str.len();

        // Symbol code
        out[pos] = self.symbol_code;
        pos += 1;

        // Comment
        if self.comment_len > 0 {
            let c_len = (self.comment_len as usize).min(self.comment.len());
            out[pos..pos + c_len].copy_from_slice(&self.comment[..c_len]);
            pos += c_len;
        }

        pos
    }
}
