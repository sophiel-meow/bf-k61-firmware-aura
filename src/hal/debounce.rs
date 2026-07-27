pub struct Debouncer {
    stable: bool,
    candidate: bool,
    count: u8,
}

impl Debouncer {
    const THRESHOLD: u8 = 4;

    pub fn new(initial: bool) -> Self {
        Debouncer {
            stable: initial,
            candidate: initial,
            count: Self::THRESHOLD,
        }
    }

    pub fn sample(&mut self, raw: bool) -> Option<bool> {
        if raw == self.candidate {
            if self.count < Self::THRESHOLD {
                self.count += 1;
            }
        } else {
            self.candidate = raw;
            self.count = 0;
        }

        if self.count >= Self::THRESHOLD && self.stable != self.candidate {
            self.stable = self.candidate;
            Some(self.stable)
        } else {
            None
        }
    }
}
