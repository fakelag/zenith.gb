use super::{
    envelope::Envelope,
    lengthcounter::LengthCounter,
    Channel,
};

const LENGTH_COUNTER_INIT: u16 = 64;

const DUTY_WAVE: [u8; 4] = [
    0b00000001, // 12.5%
    0b10000001, // 25%
    0b10000111, // 50%
    0b01111110, // 75%
];

pub struct Channel2 {
    freq_timer: u16,

    envelope: Envelope,
    length_counter: LengthCounter,

    is_enabled: bool,
    duty_cycle: u8,

    // nr21
    reg_waveduty: u8,
    reg_frequency: u16,

    // nr23
    reg_dac_enable: bool,

    sample: u8,
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
            freq_timer: 0,
            duty_cycle: 0,
            reg_frequency: 0,
            envelope: Envelope::new(),
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT),
            is_enabled: false,
            reg_waveduty: 0,
            reg_dac_enable: false,
            sample: 0,
        }
    }

    fn trigger(&mut self) {
        self.is_enabled = self.reg_dac_enable;

        self.freq_timer = (2048 - self.reg_frequency) * 4;

        self.envelope.trigger();
    }

    pub fn write_nr21(&mut self, data: u8) {
        self.length_counter.set_count(LENGTH_COUNTER_INIT - u16::from(data & 0x3F));
        self.reg_waveduty = (data >> 6) & 0x3;
    }

    pub fn write_nr22(&mut self, data: u8) {
        self.reg_dac_enable = data & 0xF8 != 0;
        self.envelope.write_nrx2(data);
    }

    pub fn write_nr23(&mut self, data: u8) {
        // @todo Period changes presumably are delayed on nr23 as well
        self.reg_frequency = (self.reg_frequency & 0x700) | u16::from(data);
    }

    pub fn write_nr24(&mut self, data: u8) {
        let length_enable_bit = data & 0x40 != 0;
        let trigger_bit = data & 0x80 != 0;
    
        self.reg_frequency = ((u16::from(data) & 0x7) << 8) | (self.reg_frequency & 0xFF);

        self.length_counter.update_enabled(trigger_bit, length_enable_bit);

        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        } else if trigger_bit {
            self.trigger();
        }
    }

    pub fn read_nr21(&mut self) -> u8 {
        (self.reg_waveduty << 6) | 0x3F
    }

    pub fn read_nr22(&mut self) -> u8 {
        self.envelope.read_nrx2()
    }

    pub fn read_nr23(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr24(&mut self) -> u8 {
        let length_bit = if self.get_length_counter().is_enabled() {
            0x40
        } else {
            0
        };
        0xBF | length_bit
    }
}

impl Channel for Channel2 {
    fn step(&mut self) {
        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        }

        self.freq_timer = self.freq_timer.saturating_sub(1);

        if self.freq_timer != 0 {
            return;
        }

        self.freq_timer = (2048 - self.reg_frequency) * 4;

        if !self.is_enabled() {
            self.sample = 0;
            return;
        }

        let wave_sample: u8 = self.envelope.get_volume() *
            ((DUTY_WAVE[self.reg_waveduty as usize] >> self.duty_cycle) & 0x1);

        self.sample = wave_sample;
        self.duty_cycle = (self.duty_cycle + 1) & 0x7;
    }

    fn get_sample(&self) -> u8 {
        self.sample
    }

    fn get_length_counter(&mut self) -> &mut LengthCounter {
        &mut self.length_counter
    }

    fn get_envelope(&mut self) -> Option<&mut super::envelope::Envelope> {
        Some(&mut self.envelope)
    }

    fn is_enabled(&self) -> bool {
        self.is_enabled && self.reg_dac_enable
    }
}
