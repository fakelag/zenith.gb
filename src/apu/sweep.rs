pub struct Sweep {
    period_timer: u8,
    enabled: bool,
    negated_since_trigger: bool,

    period: u8,
    negate: bool,
    shift: u8,

    shadow_frequency: u16,
    reg_frequency: u16,
}

impl Sweep {
    pub fn new() -> Self {
        Self {
            period_timer: 0,
            enabled: false,
            negate: false,
            period: 0,
            shift: 0,
            shadow_frequency: 0,
            reg_frequency: 0,
            negated_since_trigger: false,
        }
    }

    pub fn clock(&mut self) -> bool {
        if !self.enabled {
            return false;
        }

        self.period_timer -= 1;

        if self.period_timer != 0 {
            return false;
        }

        self.reload_timer();

        if self.period == 0 {
            return false;
        }

        let next_freq = self.calc_frequency();

        if self.overflow_check(next_freq) {
            return true;
        }

        if self.shift == 0 {
            return false;
        }

        self.shadow_frequency = next_freq;
        self.reg_frequency = next_freq;

        let check_freq = self.calc_frequency();
        return self.overflow_check(check_freq);
    }

    pub fn write_nr10(&mut self, data: u8) -> bool {
        self.period = (data >> 4) & 0x7;
        self.negate = (data >> 3) & 0x1 != 0;
        self.shift = data & 0x7;

        return !self.negate && self.negated_since_trigger;
    }

    pub fn read_nr10(&mut self) -> u8 {
        self.period << 4 | if self.negate { 1 << 3 } else { 0 } | self.shift | 0x80
    }

    pub fn trigger(&mut self) -> bool {
        self.negated_since_trigger = false;
        self.shadow_frequency = self.reg_frequency;
        self.enabled = self.period != 0 || self.shift != 0;
        self.reload_timer();

        if self.shift != 0 {
            let freq = self.calc_frequency();
            return self.overflow_check(freq);
        }
        return false;
    }

    pub fn set_frequency_msb(&mut self, freq_msb: u8) {
        self.reg_frequency = ((u16::from(freq_msb) & 0x7) << 8) | (self.reg_frequency & 0xFF);
    }

    pub fn set_frequency_lsb(&mut self, freq_lsb: u8) {
        self.reg_frequency = (self.reg_frequency & 0x700) | u16::from(freq_lsb);
    }

    pub fn get_frequency(&self) -> u16 {
        self.reg_frequency
    }

    pub fn shutdown(&mut self) {
        self.enabled = false;
        self.period_timer = 0;
        self.negated_since_trigger = false;
        self.period = 0;
        self.negate = false;
        self.shift = 0;
        self.shadow_frequency = 0;
        self.reg_frequency = 0;
    }

    fn calc_frequency(&mut self) -> u16 {
        let mut frequency_offset: i16 = (self.shadow_frequency >> self.shift) as i16;

        if self.negate {
            frequency_offset = -frequency_offset;
            self.negated_since_trigger = true;
        }

        ((self.shadow_frequency as i16) + frequency_offset) as u16
    }

    fn overflow_check(&mut self, frequency: u16) -> bool {
        frequency > 2047
    }

    fn reload_timer(&mut self) {
        self.period_timer = if self.period == 0 { 8 } else { self.period };
    }
}
