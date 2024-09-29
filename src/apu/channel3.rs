use crate::util::util;

use super::{lengthcounter::LengthCounter, Channel};

const LENGTH_COUNTER_INIT: u16 = 256;

pub struct Channel3 {
    freq_timer: u16,

    length_counter: LengthCounter,

    is_enabled: bool,
    sample_index: usize,

    nr30: u8,
    nr31: u8,
    nr32: u8,
    nr33: u8,
    nr34: u8,

    wave_ram: [u8; 16],
    sample: u8,
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            freq_timer: 0,
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT),
            sample_index: 1,
            is_enabled: false,
            nr30: 0x7F,
            nr31: 0xFF,
            nr32: 0x9F,
            nr33: 0xFF,
            nr34: 0xBF,
            wave_ram: [0; 16],
            sample: 0,
        }
    }

    fn dac_enabled(&self) -> bool {
        self.nr30 & 0x80 != 0
    }

    fn trigger(&mut self) {
        // @todo - Triggering while reading sample from wave ram corrupts the data
        self.is_enabled = self.dac_enabled();

        // self.freq_timer = 6; // @todo - Check blargg test 09 wave read while on
        // Trigger event: Frequency timer is reloaded with period.
        let frequency = util::value(self.nr34 & 0x7, self.nr33);
        self.freq_timer = (2048 - frequency) * 2;

        // Trigger event: Wave channel's position is set to 0 but sample buffer is NOT refilled.
        self.sample_index = 0;
    }

    pub fn write_nr30(&mut self, data: u8) {
        let dac_enable = data & 0x80;
        let ro_bits = self.nr30 & 0x7F;
        self.nr30 = dac_enable | ro_bits;

        if dac_enable == 0 {
            self.is_enabled = false;
        }
    }

    pub fn write_nr31(&mut self, data: u8) {
        self.length_counter.set_count(LENGTH_COUNTER_INIT - u16::from(data));
        self.nr31 = data;
    }

    pub fn write_nr32(&mut self, data: u8) {
        let ro_bits = self.nr32 & 0x9F;
        self.nr32 = (data & 0x60) | ro_bits;
    }

    pub fn write_nr33(&mut self, data: u8) {
        // @todo Period changes (written to NR33 or NR34) only take effect after the following time wave RAM is read.
        // https://gbdev.io/pandocs/Audio_Registers.html#ff1d--nr33-channel-3-period-low-write-only
        self.nr33 = data;
    }

    pub fn write_nr34(&mut self, data: u8) {
        let length_enable_current = self.length_counter.is_enabled();
        let length_enable_next = data & 0x40 != 0;

        if !length_enable_current && length_enable_next {
            self.length_counter.reset();
        }

        self.length_counter.set_enabled(length_enable_next);

        self.nr34 = data & 0xC7;

        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        } else if data & 0x80 != 0 {
            self.trigger();
        }
    }

    pub fn write_wave_ram(&mut self, addr: usize, data: u8) {
        if self.is_enabled() {
            // @todo timing behaviors
            return;
        }
        self.wave_ram[addr & 0xF] = data;
    }

    pub fn read_nr30(&mut self) -> u8 {
        self.nr30
    }

    pub fn read_nr31(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr32(&mut self) -> u8 {
        self.nr32
    }

    pub fn read_nr33(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr34(&mut self) -> u8 {
        // All other bits RO & set to 1 than length enable (bit 6)
        self.nr34 | 0xBF
    }

    pub fn read_wave_ram(&mut self, addr: usize) -> u8 {
        // @todo - Wave RAM can only be properly accessed when the channel is disabled (see obscure behavior).
        // https://gbdev.gg8.se/wiki/articles/Gameboy_sound_hardware
        if self.is_enabled() {
            // @todo timing behaviors
            return 0xFF;
        }
        self.wave_ram[addr & 0xF]
    }
}

impl Channel for Channel3 {
    fn step(&mut self) {
        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        }

        self.freq_timer = self.freq_timer.saturating_sub(1);

        if self.freq_timer != 0 {
            return;
        }

        let frequency = util::value(self.nr34 & 0x7, self.nr33);
        self.freq_timer = (2048 - frequency) * 2;

        if !self.is_enabled() {
            self.sample = 0;
            return;
        }

        let mut wave_sample: u8 = self.wave_ram[self.sample_index / 2];

        wave_sample = if self.sample_index % 2 == 0 {
            wave_sample >> 4
        } else {
            wave_sample & 0xF
        };

        let volume = (self.nr32 & 0x60) >> 5;
        wave_sample = match volume {
            0 => wave_sample >> 4,
            _ => wave_sample >> (volume - 1),
        };

        self.sample = wave_sample;
        self.sample_index = (self.sample_index + 1) % 32;
    }

    fn get_sample(&self) -> u8 {
        self.sample
    }

    fn get_length_counter(&mut self) -> &mut LengthCounter {
        &mut self.length_counter
    }

    fn is_enabled(&self) -> bool {
        self.is_enabled && self.dac_enabled()
    }
}
