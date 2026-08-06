use super::uptime;

/// `uptime::now()` ticks (100us each) per period. Kept in the same unit as
/// `uptime` so `tick()` never needs to divide.
const PERIOD_50MS: u16 = 500;
const PERIOD_100MS: u16 = 1000;
const PERIOD_500MS: u16 = 5000;

pub struct Scheduler {
    last: u16,
    acc_50: u16,
    acc_100: u16,
    acc_500: u16,
}

#[derive(Clone, Copy, Default)]
pub struct Due {
    pub every_50ms: bool,
    pub every_100ms: bool,
    pub every_500ms: bool,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            last: uptime::now(),
            acc_50: 0,
            acc_100: 0,
            acc_500: 0,
        }
    }

    pub fn tick(&mut self) -> Due {
        let now = uptime::now();
        let elapsed = now.wrapping_sub(self.last);
        self.last = now;

        let mut due = Due::default();

        self.acc_50 = self.acc_50.wrapping_add(elapsed);
        if self.acc_50 >= PERIOD_50MS {
            self.acc_50 -= PERIOD_50MS;
            due.every_50ms = true;
        }
        self.acc_100 = self.acc_100.wrapping_add(elapsed);
        if self.acc_100 >= PERIOD_100MS {
            self.acc_100 -= PERIOD_100MS;
            due.every_100ms = true;
        }
        self.acc_500 = self.acc_500.wrapping_add(elapsed);
        if self.acc_500 >= PERIOD_500MS {
            self.acc_500 -= PERIOD_500MS;
            due.every_500ms = true;
        }

        due
    }
}
