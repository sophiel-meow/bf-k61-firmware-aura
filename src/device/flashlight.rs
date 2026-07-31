use crate::board;
use kd32f328_pac::gpiof;

pub struct Flashlight {
    gpiob: &'static gpiof::RegisterBlock,
    on: bool,
}

impl Flashlight {
    pub fn new(gpiob: &'static gpiof::RegisterBlock) -> Self {
        board::init_flashlight_led(gpiob);
        Flashlight {
            gpiob,
            on: false,
        }
    }

    pub fn toggle(&mut self) {
        self.on = !self.on;
        board::set_flashlight_led(self.gpiob, self.on);
    }
}
