
pub struct LengthCounter {
    initial_count: u16,
    count: u16,
    enabled: bool,
}

impl LengthCounter {
    pub fn new(initial_count: u16) -> Self {
        Self {
            initial_count,
            count: 1,
            enabled: false,
        }
    }

    pub fn step(&mut self) {
        if self.enabled && self.count != 0 {
            self.count = self.count.saturating_sub(1);
        }
    }

    pub fn update_enabled(&mut self, _trigger_bit: bool, enable_bit: bool) {
        // @todo - Check clocking rules for length timer when writing to NRx4
        if !self.enabled && enable_bit {
            self.reset();
        }
        self.enabled = enable_bit;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get_count(&self) -> u16 {
        self.count
    }

    pub fn set_count(&mut self, count: u16) {
        self.count = count;
    }

    pub fn reset(&mut self) {
        self.count = self.initial_count;
    }
}
