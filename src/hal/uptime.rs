use crate::hal::clock::SYSCLK_HZ;

const TICK_HZ: u32 = 10_000;
const PSC: u16 = ((SYSCLK_HZ + TICK_HZ / 2) / TICK_HZ - 1) as u16;

pub fn init() {
    let tim = unsafe { &*kd32f328_pac::Tim14::ptr() };
    tim.cr1().write(|w| w.cen().clear_bit());
    tim.psc().write(|w| unsafe { w.bits(PSC.into()) });
    tim.arr().write(|w| unsafe { w.bits(0xFFFF) });
    tim.egr().write(|w| w.ug().set_bit());
    tim.cr1().write(|w| w.cen().set_bit());
}

/// Current tick count, in units of 100us. Wraps every 6.5536s.
pub fn now() -> u16 {
    let tim = unsafe { &*kd32f328_pac::Tim14::ptr() };
    tim.cnt().read().bits() as u16
}
