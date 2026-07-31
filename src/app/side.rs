use super::{power_from_raw, power_to_raw, subaudio_from_code, subaudio_to_code, ChVfoMode};
use crate::device::radio::{ChannelConfig, Power, SubAudio};
use crate::flash_map::{self, addr};

#[derive(Clone)]
pub(super) struct Side {
    pub vfo_chan: ChVfoMode,
    pub channel_num: u16,
    pub freq_step: u8,
    pub rx_freq_hz: u32,
    pub tx_freq_hz: u32,
    pub freq_dir: u8,
    pub offset_hz: u32,
    pub reversed: bool,
    pub cfg: ChannelConfig,
    /// Raw 12-byte channel name (only meaningful when `vfo_chan ==
    /// Channel`); `[0; 12]` in VFO mode or for an unnamed channel.
    pub name: [u8; 12],
    /// Snapshot of `(rx_freq_hz, freq_dir, offset_hz)` taken the moment
    /// this side leaves VFO mode, since Channel mode reuses those same
    /// fields: without this, switching back to VFO mode has nothing left
    /// to restore and keeps showing the channel's frequency. Restored
    /// verbatim on the way back to `Vfo`; only ever written on the way out
    /// of it (see `App::toggle_vfo_channel`).
    pub vfo_backup: (u32, u8, u32),
}

impl Side {
    pub fn refresh_cfg_freqs(&mut self) {
        let (tx, rx) = match self.vfo_chan {
            ChVfoMode::Vfo => {
                let tx = match self.freq_dir {
                    1 => self.rx_freq_hz.saturating_add(self.offset_hz),
                    2 => self.rx_freq_hz.saturating_sub(self.offset_hz),
                    _ => self.rx_freq_hz,
                };
                (tx, self.rx_freq_hz)
            }
            ChVfoMode::Channel => (self.tx_freq_hz, self.rx_freq_hz),
        };
        if self.reversed {
            self.cfg.freq_hz = tx;
            self.cfg.tx_freq_hz = rx;
        } else {
            self.cfg.freq_hz = rx;
            self.cfg.tx_freq_hz = tx;
        }
    }

    pub fn load_vfo(&mut self, vfo: &flash_map::VfoMode) {
        self.vfo_chan = ChVfoMode::Vfo;
        self.name = [0; 12];
        self.rx_freq_hz = vfo.freq_deci_hz() * 10;
        self.freq_dir = vfo.freq_dir();
        self.offset_hz = vfo.offset_deci_hz() * 10;
        self.vfo_backup = (self.rx_freq_hz, self.freq_dir, self.offset_hz);
        self.cfg.wide_band = !vfo.wide_narrow();
        self.cfg.power = power_from_raw(vfo.tx_power);
        self.cfg.subaudio_tx = subaudio_from_code(vfo.tx_dcs_cts_num);
        self.cfg.subaudio_rx = subaudio_from_code(vfo.rx_dcs_cts_num);
        self.refresh_cfg_freqs();
    }

    pub fn to_vfo_bytes(&self) -> [u8; addr::VFO_SIZE as usize] {
        let mut vfo = flash_map::VfoMode::from_bytes(&[0u8; addr::VFO_SIZE as usize]);
        vfo.set_freq_deci_hz(self.rx_freq_hz / 10);
        vfo.set_offset_deci_hz(self.offset_hz / 10);
        vfo.set_wide_narrow(!self.cfg.wide_band);
        vfo.tx_power = power_to_raw(self.cfg.power);
        vfo.rx_dcs_cts_num = subaudio_to_code(self.cfg.subaudio_rx);
        vfo.tx_dcs_cts_num = subaudio_to_code(self.cfg.subaudio_tx);
        vfo.dtmf_group = (self.freq_dir << 5) | (vfo.dtmf_group & 0x1f);
        vfo.to_bytes()
    }

    pub fn load_channel(&mut self, num: u16, ch: &flash_map::Channel) {
        self.vfo_chan = ChVfoMode::Channel;
        self.channel_num = num;
        self.rx_freq_hz = ch.rx_freq_deci_hz() * 10;
        self.tx_freq_hz = ch.tx_freq_deci_hz() * 10;
        self.cfg.wide_band = !ch.wide_narrow();
        self.cfg.power = power_from_raw(ch.tx_power);
        self.cfg.subaudio_tx = subaudio_from_code(ch.tx_dcs_cts_num);
        self.cfg.subaudio_rx = subaudio_from_code(ch.rx_dcs_cts_num);
        self.name = ch.name;
        self.refresh_cfg_freqs();
    }

    pub fn step_deci_hz(&self) -> u32 {
        super::STEP_LIST_DECI_HZ[self.freq_step as usize]
    }
}
