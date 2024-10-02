
pub struct LengthCounter {
    frame_seq_step: u8,
    initial_count: u16,
    count: u16,
    enabled: bool,
}

impl LengthCounter {
    pub fn new(initial_count: u16) -> Self {
        Self {
            frame_seq_step: 0,
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

    pub fn update_enabled(&mut self, trigger_bit: bool, enable_bit: bool) {
        let reset_length = self.initial_count - (if self.enabled {
            (self.frame_seq_step & 1) as u16
        } else {
            0
        });

        let reset: bool = trigger_bit && self.count == 0;

        if !self.enabled && !enable_bit && reset {
            self.count = reset_length;
        } else if self.enabled && self.frame_seq_step & 1 != 0 {
            self.count = self.count.saturating_sub(1);

            if trigger_bit && self.count == 0 {
                self.count = self.initial_count - 1;
            }
        } else if reset {
            self.count = reset_length;
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

    pub fn update_frame_sequencer_step(&mut self, step: u8) {
        self.frame_seq_step = step;
    }

    pub fn shutdown(&mut self) {
        self.enabled = false;
        self.frame_seq_step = 0;

        // @todo CGB: resets count to initial
        // self.count = 0;
    }
}
