use kd32f328_pac::tim6;

/// 1200 Hz bit-clock backed by TIM6.
///
/// The timer is configured for a 1 us tick (96 MHz / 96) with an
/// auto-reload of 832, giving one update event every 833 us = 1200 Hz.
pub struct AfskTimer {
    tim6: &'static tim6::RegisterBlock,
}

impl AfskTimer {
    pub unsafe fn new() -> Self {
        let tim6 = unsafe { &*kd32f328_pac::Tim6::ptr() };
        tim6.cr1().write(|w| w.cen().clear_bit());
        tim6.psc().write(|w| unsafe { w.bits(95) });
        tim6.arr().write(|w| unsafe { w.bits(832) });
        tim6.egr().write(|w| w.ug().set_bit());
        tim6.sr().write(|w| w.uif().clear_bit());
        AfskTimer { tim6 }
    }

    pub fn sync_start(&self) {
        self.tim6.egr().write(|w| w.ug().set_bit());
        self.tim6.sr().write(|w| w.uif().clear_bit());
        self.tim6.cr1().write(|w| w.cen().set_bit());
    }

    pub fn wait_bit(&self) {
        while self.tim6.sr().read().uif().bit_is_clear() {}
        self.tim6.sr().write(|w| w.uif().clear_bit());
    }

    pub fn stop(&self) {
        self.tim6.cr1().write(|w| w.cen().clear_bit());
    }
}
