use super::{
    envelope::Envelope,
    lengthcounter::LengthCounter,
    sweep::Sweep,
    Channel,
};

const LENGTH_COUNTER_INIT: u16 = 64;

const DUTY_WAVE: [u8; 4] = [
    0b00000001, // 12.5%
    0b10000001, // 25%
    0b10000111, // 50%
    0b01111110, // 75%
];

pub struct Channel1 {
    freq_timer: u16,

    sweep: Sweep,
    envelope: Envelope,
    length_counter: LengthCounter,

    is_enabled: bool,
    duty_cycle: u8,

    // nr11
    reg_waveduty: u8,

    // nr13
    reg_dac_enable: bool,

    sample: u8,
}

impl Channel1 {
    pub fn new() -> Self {
        Self {
            freq_timer: 0,
            duty_cycle: 0,
            envelope: Envelope::new(0xF, false, 3),
            sweep: Sweep::new(),
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT),
            is_enabled: true,
            reg_waveduty: 2,
            reg_dac_enable: true,
            sample: 0,
        }
    }

    pub fn sweep_step(&mut self) {
        let is_overflow = self.sweep.step();

        if is_overflow {
            self.is_enabled = false;
        }
    }

    fn trigger(&mut self) {
        self.is_enabled = self.reg_dac_enable;

        self.freq_timer = (2048 - self.sweep.get_frequency()) * 4;

        self.envelope.trigger();

        let is_overflow = self.sweep.trigger();

        if is_overflow {
            self.is_enabled = false;
        }
    }

    pub fn write_nr10(&mut self, data: u8) {
        self.sweep.write_nr10(data);
        // @todo - Check if writing to nr10 can overflow sweep in some cases
    }

    pub fn write_nr11(&mut self, data: u8) {
        self.length_counter.set_count(LENGTH_COUNTER_INIT - u16::from(data & 0x3F));
        self.reg_waveduty = (data >> 6) & 0x3;
    }

    pub fn write_nr12(&mut self, data: u8) {
        self.reg_dac_enable = data & 0xF8 != 0;

        if !self.reg_dac_enable {
            self.is_enabled = false;
        }

        self.envelope.write_nrx2(data);
    }

    pub fn write_nr13(&mut self, data: u8) {
        // @todo Period changes (written to NR13 or NR14) only take effect after the current “sample” ends; see description above.
        // https://gbdev.io/pandocs/Audio_Registers.html#ff13--nr13-channel-1-period-low-write-only
        self.sweep.set_frequency_lsb(data);
    }

    pub fn write_nr14(&mut self, data: u8) {
        let length_enable_bit = data & 0x40 != 0;
        let trigger_bit = data & 0x80 != 0;
    
        self.sweep.set_frequency_msb(data);

        self.length_counter.write_nrx4(trigger_bit, length_enable_bit);

        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        } else if trigger_bit {
            self.trigger();
        }
    }

    pub fn read_nr10(&mut self) -> u8 {
        self.sweep.read_nr10()
    }

    pub fn read_nr11(&mut self) -> u8 {
        (self.reg_waveduty << 6) | 0x3F
    }

    pub fn read_nr12(&mut self) -> u8 {
        self.envelope.read_nrx2()
    }

    pub fn read_nr13(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr14(&mut self) -> u8 {
        let length_bit = if self.get_length_counter().is_enabled() {
            0x40
        } else {
            0
        };
        0xBF | length_bit
    }
}

impl Channel for Channel1 {
    fn step(&mut self) {
        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        }

        self.freq_timer = self.freq_timer.saturating_sub(1);

        if self.freq_timer != 0 {
            return;
        }

        self.freq_timer = (2048 - self.sweep.get_frequency()) * 4;

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

    fn shutdown(&mut self) {
        self.is_enabled = false;
        self.reg_dac_enable = false;
        self.duty_cycle = 0;
        self.reg_waveduty = 0;
        self.sweep.shutdown();
        self.envelope.shutdown();
        self.length_counter.shutdown();
    }
}
