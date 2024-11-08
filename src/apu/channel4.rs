use crate::GbCtx;

use super::{envelope::Envelope, lengthcounter::LengthCounter, Channel};

const LENGTH_COUNTER_INIT: u16 = 64;

const DIVISORS: [u8; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

pub struct Channel4 {
    freq_timer: u16,

    length_counter: LengthCounter,
    envelope: Envelope,

    is_enabled: bool,
    lfsr: u16,

    reg_dac_enable: bool,
    reg_clock_shift: u8,
    reg_lfsr_7bit: bool,
    reg_divisor: u8,

    sample: u8,
}

impl Channel4 {
    pub fn new(ctx: std::rc::Rc<GbCtx>) -> Self {
        Self {
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT, ctx),
            envelope: Envelope::new(0, false, 0),
            freq_timer: 0,
            lfsr: 0,
            reg_clock_shift: 0,
            reg_divisor: 0,
            reg_lfsr_7bit: false,
            is_enabled: false,
            reg_dac_enable: false,
            sample: 0,
        }
    }

    fn trigger(&mut self) {
        self.is_enabled = self.reg_dac_enable;

        self.freq_timer = self.calc_freq();
        self.lfsr = 0;

        self.envelope.trigger();
    }

    pub fn write_nr41(&mut self, data: u8) {
        self.length_counter
            .set_count(LENGTH_COUNTER_INIT - u16::from(data & 0x3F));
    }

    pub fn write_nr42(&mut self, data: u8) {
        self.reg_dac_enable = data & 0xF8 != 0;

        if !self.reg_dac_enable {
            self.is_enabled = false;
        }

        self.envelope.write_nrx2(data);
    }

    pub fn write_nr43(&mut self, data: u8) {
        self.reg_clock_shift = (data >> 4) & 0xF;
        self.reg_lfsr_7bit = data & 0x8 != 0;
        self.reg_divisor = data & 0x7;
    }

    pub fn write_nr44(&mut self, data: u8) {
        let length_enable_bit = data & 0x40 != 0;
        let trigger_bit = data & 0x80 != 0;

        self.length_counter
            .write_nrx4(trigger_bit, length_enable_bit);

        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        } else if trigger_bit {
            self.trigger();
        }
    }

    pub fn read_nr41(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr42(&mut self) -> u8 {
        self.envelope.read_nrx2()
    }

    pub fn read_nr43(&mut self) -> u8 {
        self.reg_clock_shift << 4 | if self.reg_lfsr_7bit { 1 << 3 } else { 0 } | self.reg_divisor
    }

    pub fn read_nr44(&mut self) -> u8 {
        let length_bit = (self.length_counter.is_enabled() as u8) << 6;
        0xBF | length_bit
    }

    fn calc_freq(&self) -> u16 {
        let divisor = DIVISORS[self.reg_divisor as usize] as u16;
        divisor << self.reg_clock_shift
    }
}

impl Channel for Channel4 {
    fn clock(&mut self) {
        if self.freq_timer > 1 {
            self.freq_timer -= 1;
            return;
        }

        self.freq_timer = self.calc_freq();

        if !self.is_enabled() {
            self.sample = 0;
            return;
        }

        let xor_result = ((self.lfsr & 0x1) ^ ((self.lfsr >> 1) & 0x1)) ^ 1;

        self.lfsr = (self.lfsr & 0x7FFF) | (xor_result << 15);

        if self.reg_lfsr_7bit {
            self.lfsr = (self.lfsr & 0xFF7F) | (xor_result << 7);
        }

        self.lfsr >>= 1;
        self.sample = self.envelope.get_volume() * (self.lfsr & 0x1) as u8;
    }

    fn length_clock(&mut self) {
        if self.length_counter.clock() {
            self.is_enabled = false;
        }
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
        self.reg_clock_shift = 0;
        self.reg_divisor = 0;
        self.reg_lfsr_7bit = false;
        self.length_counter.shutdown();
        self.envelope.shutdown();
    }
}
