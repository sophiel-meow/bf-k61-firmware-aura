use super::{power_from_raw, power_to_raw, subaudio_from_code, subaudio_to_code, ChVfoMode};
use crate::device::radio::{ChannelConfig, Power, SubAudio};
use crate::flash_map::{self, addr};

#[derive(Clone, Copy)]
pub(super) struct VfoBackup {
    pub rx_freq_hz: u32,
    pub freq_dir: u8,
    pub offset_hz: u32,
    pub wide_band: bool,
    pub power: Power,
    pub subaudio_tx: SubAudio,
    pub subaudio_rx: SubAudio,
    pub ani_target: Option<u8>,
}

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
    /// Default ANI call target for this channel/VFO (a contact-table
    /// index), used when there's no runtime override selected. Loaded from
    /// `Channel`/`VfoMode`'s `dtmf_group` low 5 bits.
    pub ani_target: Option<u8>,
    pub cfg: ChannelConfig,
    /// Raw 12-byte channel name (only meaningful when `vfo_chan ==
    /// Channel`); `[0; 12]` in VFO mode or for an unnamed channel.
    pub name: [u8; 12],
    pub vfo_backup: VfoBackup,
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
        self.ani_target = vfo.ani_target();
        self.offset_hz = vfo.offset_deci_hz() * 10;
        self.cfg.wide_band = !vfo.wide_narrow();
        self.cfg.power = power_from_raw(vfo.tx_power);
        self.cfg.subaudio_tx = subaudio_from_code(vfo.tx_dcs_cts_num);
        self.cfg.subaudio_rx = subaudio_from_code(vfo.rx_dcs_cts_num);
        self.vfo_backup = VfoBackup {
            rx_freq_hz: self.rx_freq_hz,
            freq_dir: self.freq_dir,
            offset_hz: self.offset_hz,
            wide_band: self.cfg.wide_band,
            power: self.cfg.power,
            subaudio_tx: self.cfg.subaudio_tx,
            subaudio_rx: self.cfg.subaudio_rx,
            ani_target: self.ani_target,
        };
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
        vfo.set_freq_dir(self.freq_dir);
        vfo.set_ani_target(self.ani_target);
        vfo.to_bytes()
    }

    pub fn load_channel(&mut self, num: u16, ch: &flash_map::Channel) {
        self.vfo_chan = ChVfoMode::Channel;
        self.channel_num = num;
        self.rx_freq_hz = ch.rx_freq_deci_hz() * 10;
        self.tx_freq_hz = ch.tx_freq_deci_hz() * 10;
        self.ani_target = ch.ani_target();
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
