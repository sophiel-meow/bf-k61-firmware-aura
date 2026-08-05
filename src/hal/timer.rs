use kd32f328_pac::tim6;

use crate::hal::clock::SYSCLK_HZ;

/// Target TIM6 tick rate: 1 tick/us
const TICK_HZ: u32 = 1_000_000;

const PSC: u16 = ((SYSCLK_HZ + TICK_HZ / 2) / TICK_HZ - 1) as u16;

const SSTV_TICK_HZ: u32 = 200_000;

const SSTV_PSC: u16 = ((SYSCLK_HZ + SSTV_TICK_HZ / 2) / SSTV_TICK_HZ - 1) as u16;

/// 1200 Hz bit-clock backed by TIM6.
pub struct AfskTimer {
    tim6: &'static tim6::RegisterBlock,
}

impl AfskTimer {
    pub unsafe fn new() -> Self {
        let tim6 = unsafe { &*kd32f328_pac::Tim6::ptr() };
        tim6.cr1().write(|w| w.cen().clear_bit());
        tim6.psc().write(|w| unsafe { w.bits(PSC.into()) });
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

    pub unsafe fn new_sstv_line_clock(auto_reload: u32) -> Self {
        let tim6 = unsafe { &*kd32f328_pac::Tim6::ptr() };
        tim6.cr1().write(|w| w.cen().clear_bit());
        tim6.psc().write(|w| unsafe { w.bits(SSTV_PSC.into()) });
        tim6.arr().write(|w| unsafe { w.bits(auto_reload) });
        tim6.egr().write(|w| w.ug().set_bit());
        tim6.sr().write(|w| w.uif().clear_bit());
        AfskTimer { tim6 }
    }

    pub fn count(&self) -> u32 {
        self.tim6.cnt().read().bits()
    }

    pub fn wait_until(&self, tick: u32) {
        while self.count() < tick {}
    }

    pub fn wait_wrap(&self) {
        while self.tim6.sr().read().uif().bit_is_clear() {}
        self.tim6.sr().write(|w| w.uif().clear_bit());
    }
}
