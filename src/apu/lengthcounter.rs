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

    pub fn clock(&mut self) -> bool {
        if self.enabled && self.count != 0 {
            self.count -= 1;
        }
        return self.enabled && self.count == 0;
    }

    fn write_nrx4_enabled(&mut self, trigger_bit: bool, enable_bit: bool) {
        if !trigger_bit || self.count != 0 {
            return;
        }

        self.count = self.initial_count;

        if enable_bit {
            self.count -= (self.frame_seq_step & 1) as u16;
        }
    }

    fn write_nrx4_disabled_to_enabled(&mut self, trigger_bit: bool) {
        if self.frame_seq_step & 1 == 0 {
            return;
        }

        self.count = self.count.saturating_sub(1);

        if self.count == 0 && trigger_bit {
            self.count = self.initial_count - 1;
        }
    }

    fn write_nrx4_disabled(&mut self, trigger_bit: bool) {
        if !trigger_bit || self.count != 0 {
            return;
        }

        self.count = self.initial_count;
    }

    pub fn write_nrx4(&mut self, trigger_bit: bool, enable_bit: bool) {
        if self.enabled {
            self.write_nrx4_enabled(trigger_bit, enable_bit);
        } else if enable_bit {
            self.write_nrx4_disabled_to_enabled(trigger_bit);
        } else {
            self.write_nrx4_disabled(trigger_bit);
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
        // self.count = self.initial_count;
    }
}
