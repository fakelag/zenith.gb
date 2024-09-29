
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
        if !self.enabled && self.count != 0 {
            return;
        }

        self.count = self.count.saturating_sub(1);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
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
