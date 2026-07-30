use super::settings;
use super::{App, ChVfoMode, Mode, SearchStatus};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::{self, RawTone, SubAudio};
use cortex_m::peripheral::SYST;

const TONE_TIMEOUT_TICKS: u16 = 50;
const SAMPLE_AGREEMENT_MAX: u32 = 25;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchBand {
    Uhf,
    Vhf,
    Band200M,
    Band350M,
}

impl SearchBand {
    fn next(self) -> Self {
        match self {
            SearchBand::Uhf => SearchBand::Vhf,
            SearchBand::Vhf => SearchBand::Band200M,
            SearchBand::Band200M => SearchBand::Band350M,
            SearchBand::Band350M => SearchBand::Uhf,
        }
    }

    fn uhf_path(self) -> bool {
        matches!(self, SearchBand::Uhf | SearchBand::Band350M)
    }

    fn label(self) -> &'static str {
        match self {
            SearchBand::Uhf => "UHF",
            SearchBand::Vhf => "VHF",
            SearchBand::Band200M => "220M",
            SearchBand::Band350M => "350M",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchPhase {
    Setup,
    Hunt,
    Check,
    ToneSetup,
    ToneWait,
    Found,
}

pub(super) struct SearchState {
    band: SearchBand,
    phase: SearchPhase,
    sample: Option<u32>,
    /// Candidate frequency, in raw deci-Hz words
    found_freq_word: u32,
    tone: Option<SubAudio>,
    timeout_ticks: u16,
}

impl SearchState {
    pub(super) const fn new() -> Self {
        SearchState {
            band: SearchBand::Uhf,
            phase: SearchPhase::Setup,
            sample: None,
            found_freq_word: 0,
            tone: None,
            timeout_ticks: 0,
        }
    }
}

fn correct_band_fold(band: SearchBand, freq: u32) -> Option<u32> {
    let mut freq = freq;
    match band {
        SearchBand::Uhf => {
            if freq > 52_000_000 {
                freq /= 2;
            }
            if freq < 40_000_000 {
                return None;
            }
        }
        SearchBand::Vhf => {
            if freq >= 36_000_000 {
                freq /= 3;
            } else if freq > 25_000_000 {
                freq /= 2;
            } else if freq < 10_000_000 {
                return None;
            }
            if freq > 20_000_000 {
                return None;
            }
        }
        SearchBand::Band200M => {
            if freq >= 40_000_000 {
                freq /= 2;
            } else if freq < 20_000_000 {
                return None;
            }
            if freq > 30_000_000 {
                return None;
            }
        }
        SearchBand::Band350M => {
            if freq >= 70_000_000 {
                freq /= 2;
            } else if freq < 30_000_000 {
                return None;
            }
            if freq > 40_000_000 {
                return None;
            }
        }
    }
    Some(freq)
}

fn clamp_band(band: SearchBand, freq: u32) -> u32 {
    match band {
        SearchBand::Vhf => freq.clamp(13_600_000, 17_400_000),
        SearchBand::Band200M => freq.clamp(20_000_000, 26_000_000),
        SearchBand::Band350M => {
            if freq > 39_000_000 {
                39_000_000
            } else if freq < 30_000_000 {
                35_000_000
            } else {
                freq
            }
        }
        SearchBand::Uhf => freq.clamp(40_000_000, 52_000_000),
    }
}

pub(super) fn enter(app: &mut App, _syst: &mut SYST) {
    app.search = SearchState::new();
    app.mode = Mode::Search;
    app.input.clear();
}

fn exit(app: &mut App, syst: &mut SYST) {
    app.radio.freq_scan_disable(syst);
    app.radio.set_subaudio_scan_filter(syst, false);
    app.mode = Mode::Standby;
    app.reset_key_idle();
    app.sync_watching_to_master(syst);
}

fn save(app: &mut App, syst: &mut SYST) {
    let freq_hz = app.search.found_freq_word * 10;
    let tone = app.search.tone.unwrap_or(SubAudio::None);
    let s = &mut app.sides[app.master];
    s.vfo_chan = ChVfoMode::Vfo;
    s.rx_freq_hz = freq_hz;
    s.cfg.subaudio_rx = tone;
    s.cfg.subaudio_tx = tone;
    s.refresh_cfg_freqs();
    app.save_vfo();
    exit(app, syst);
}

pub(super) fn poll(app: &mut App, syst: &mut SYST) {
    if app.mode != Mode::Search {
        return;
    }
    match app.search.phase {
        SearchPhase::Setup => {
            app.radio.freq_scan_enable(syst);
            app.search.sample = None;
            app.search.phase = SearchPhase::Hunt;
        }
        SearchPhase::Hunt => {
            let Some(raw) = app.radio.check_freq_scan(syst) else {
                return;
            };
            app.radio.freq_scan_disable(syst);
            match app.search.sample.take() {
                None => {
                    app.search.sample = Some(raw);
                    app.radio.freq_scan_enable(syst);
                }
                Some(first) => {
                    if first.abs_diff(raw) < SAMPLE_AGREEMENT_MAX {
                        let avg = (first + raw) / 2;
                        match correct_band_fold(app.search.band, avg) {
                            Some(corrected) => {
                                app.search.found_freq_word = corrected;
                                app.search.phase = SearchPhase::Check;
                            }
                            None => app.search.phase = SearchPhase::Setup,
                        }
                    } else {
                        app.radio.freq_scan_enable(syst);
                    }
                }
            }
        }
        SearchPhase::Check => {
            let corrected = app
                .radio
                .correct_measured_freq_word(app.search.found_freq_word);
            let rounded = (corrected + 13) / 25 * 25;
            let clamped = clamp_band(app.search.band, rounded);
            app.search.found_freq_word = clamped;
            app.radio
                .tune_search_candidate(syst, clamped * 10, app.search.band.uhf_path());
            app.radio.set_subaudio_scan_filter(syst, true);
            app.search.phase = SearchPhase::ToneSetup;
        }
        SearchPhase::ToneSetup => {
            app.search.timeout_ticks = TONE_TIMEOUT_TICKS;
            app.search.tone = None;
            app.search.phase = SearchPhase::ToneWait;
        }
        SearchPhase::ToneWait => match app.radio.detect_subaudio_raw(syst) {
            RawTone::None => {
                if app.search.timeout_ticks > 0 {
                    app.search.timeout_ticks -= 1;
                } else {
                    app.search.phase = SearchPhase::Found;
                }
            }
            RawTone::Ctcss(raw) => {
                let tenths = radio::ctcss_raw_to_tenths_hz(raw);
                app.search.tone =
                    radio::find_standard_ctcss(tenths, &settings::CTCSS_TABLE).map(SubAudio::Ctcss);
                app.search.phase = SearchPhase::Found;
            }
            RawTone::Dcs(raw) => {
                app.search.tone =
                    radio::find_standard_dcs(raw, &settings::DCS_TABLE).map(|code| SubAudio::Dcs {
                        code,
                        inverted: false,
                    });
                app.search.phase = SearchPhase::Found;
            }
        },
        SearchPhase::Found => {}
    }
}

pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match (ev.kind, ev.key) {
        (KeyEventKind::Single, KeyId::Ab) => {
            app.search.band = app.search.band.next();
            app.search.phase = SearchPhase::Setup;
        }
        (KeyEventKind::Single, KeyId::Up | KeyId::Down)
            if app.search.phase == SearchPhase::Found =>
        {
            app.search.phase = SearchPhase::Setup;
        }
        (KeyEventKind::Single, KeyId::Menu) if app.search.phase == SearchPhase::Found => {
            save(app, syst);
        }
        (KeyEventKind::Single, KeyId::Exit) | (KeyEventKind::Long, KeyId::Exit) => {
            exit(app, syst);
        }
        _ => {}
    }
}

// UI getters
pub(super) fn band_label(app: &App) -> &'static str {
    app.search.band.label()
}

pub(super) fn status(app: &App) -> SearchStatus {
    match app.search.phase {
        SearchPhase::Setup | SearchPhase::Hunt | SearchPhase::Check => SearchStatus::Hunting,
        SearchPhase::ToneSetup | SearchPhase::ToneWait => SearchStatus::Listening,
        SearchPhase::Found => SearchStatus::Found,
    }
}

pub(super) fn candidate_freq_hz(app: &App) -> u32 {
    app.search.found_freq_word * 10
}

pub(super) fn tone(app: &App) -> Option<SubAudio> {
    app.search.tone
}
