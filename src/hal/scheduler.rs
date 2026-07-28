pub struct Scheduler {
    ticks_50ms: u8,
    ticks_100ms: u8,
    ticks_500ms: u8,
}

#[derive(Clone, Copy, Default)]
pub struct Due {
    pub every_50ms: bool,
    pub every_100ms: bool,
    pub every_500ms: bool,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            ticks_50ms: 0,
            ticks_100ms: 0,
            ticks_500ms: 0,
        }
    }

    pub fn tick(&mut self) -> Due {
        let mut due = Due::default();

        self.ticks_50ms += 1;
        if self.ticks_50ms >= 5 {
            self.ticks_50ms = 0;
            due.every_50ms = true;
        }

        self.ticks_100ms += 1;
        if self.ticks_100ms >= 10 {
            self.ticks_100ms = 0;
            due.every_100ms = true;
        }

        self.ticks_500ms += 1;
        if self.ticks_500ms >= 50 {
            self.ticks_500ms = 0;
            due.every_500ms = true;
        }

        due
    }
}
