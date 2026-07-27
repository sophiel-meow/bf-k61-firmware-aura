use kd32f328_pac::adc;

const SMP_239_5_CYCLES: u8 = 0b111;

pub struct Adc<'a> {
    adc: &'a adc::RegisterBlock,
}

impl<'a> Adc<'a> {
    pub fn new(adc: &'a adc::RegisterBlock) -> Self {
        adc.cr().modify(|_, w| w.adcal().set_bit());
        while adc.cr().read().adcal().bit_is_set() {}

        adc.smpr()
            .write(|w| unsafe { w.smpr().bits(SMP_239_5_CYCLES) });

        adc.cr().modify(|_, w| w.aden().set_bit());
        while adc.isr().read().adrdy().bit_is_clear() {}

        Adc { adc }
    }

    pub fn read_channel(&mut self, channel: u8) -> u16 {
        self.adc.chselr().write(|w| unsafe { w.bits(1 << channel) });
        self.adc.cr().modify(|_, w| w.adstart().set_bit());
        while self.adc.isr().read().eoc().bit_is_clear() {}
        self.adc.dr().read().data().bits()
    }
}
