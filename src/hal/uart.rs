use core::cell::RefCell;
use cortex_m::interrupt::Mutex;
use kd32f328_pac::usart1;

const USER_VECTORS_BASE: usize = 0x2000_1000;
const USER_VECTOR_USART1: usize = 1;

pub fn install_bootloader_trampoline() {
    let slot = (USER_VECTORS_BASE + USER_VECTOR_USART1 * 4) as *mut usize;
    let handler = USART1 as unsafe extern "C" fn() as usize;
    unsafe { core::ptr::write_volatile(slot, handler) };
}

const RX_RING_CAP: usize = 320;

struct RxRing {
    buf: [u8; RX_RING_CAP],
    head: usize,
    tail: usize,
    len: usize,
}

impl RxRing {
    const fn new() -> Self {
        RxRing {
            buf: [0; RX_RING_CAP],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len == RX_RING_CAP {
            self.tail = (self.tail + 1) % RX_RING_CAP;
            self.len -= 1;
        }
        self.buf[self.head] = byte;
        self.head = (self.head + 1) % RX_RING_CAP;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.buf[self.tail];
        self.tail = (self.tail + 1) % RX_RING_CAP;
        self.len -= 1;
        Some(byte)
    }
}

static RX_RING: Mutex<RefCell<RxRing>> = Mutex::new(RefCell::new(RxRing::new()));

/// USART1 IRQ handler: drains RDR into `RX_RING` on every received byte
#[no_mangle]
pub unsafe extern "C" fn USART1() {
    let regs = &*kd32f328_pac::Usart1::ptr();
    let isr = regs.isr().read();
    if isr.ore().bit_is_set() {
        regs.icr().write(|w| w.orecf().set_bit());
    }
    if isr.rxne().bit_is_set() {
        let byte = regs.rdr().read().rdr().bits() as u8;
        cortex_m::interrupt::free(|cs| RX_RING.borrow(cs).borrow_mut().push(byte));
    }
}

/// Pull one byte received on USART1 since the last call, if any.
pub fn take_byte() -> Option<u8> {
    cortex_m::interrupt::free(|cs| RX_RING.borrow(cs).borrow_mut().pop())
}

pub struct DebugUart<'a> {
    regs: &'a usart1::RegisterBlock,
}

impl<'a> DebugUart<'a> {
    /// pclk_hz: bus freq (APB2 for USART1)
    /// OVER8 == 0 (16x oversampling)
    pub fn new(regs: &'a usart1::RegisterBlock, pclk_hz: u32, baud: u32) -> Self {
        // RAW BRR = round(pclk*16/baud)；
        // high 12 bit (bit4..15): mantissa
        // low 4 bit: fraction (1/16)
        let raw = (pclk_hz as u64 * 16 + baud as u64 / 2) / baud as u64;
        let mantissa = (raw >> 4) as u16;
        let fraction = (raw & 0xF) as u8;

        regs.brr().write(|w| unsafe {
            w.div_mantissa().bits(mantissa);
            w.div_fraction().bits(fraction)
        });

        regs.cr1().write(|w| {
            w.te().set_bit();
            w.re().set_bit();
            w.rxneie().set_bit();
            w.ue().set_bit()
        });

        DebugUart { regs }
    }

    pub fn write_byte(&mut self, byte: u8) {
        while self.regs.isr().read().txe().bit_is_clear() {}
        self.regs
            .tdr()
            .write(|w| unsafe { w.tdr().bits(byte as u16) });
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }
}

impl core::fmt::Write for DebugUart<'_> {
    #[cfg(debug_assertions)]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
        Ok(())
    }

    #[cfg(not(debug_assertions))]
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        Ok(())
    }
}
