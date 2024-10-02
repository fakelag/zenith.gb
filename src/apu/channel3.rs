use super::{lengthcounter::LengthCounter, Channel};

const LENGTH_COUNTER_INIT: u16 = 256;

pub struct Channel3 {
    freq_timer: u16,

    length_counter: LengthCounter,

    is_enabled: bool,
    sample_index: usize,

    reg_dac_enable: bool,
    reg_volume: u8,
    reg_frequency: u16,

    wave_ram: [u8; 16],
    sample: u8,
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            freq_timer: 0,
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT),
            sample_index: 0,
            is_enabled: false,
            reg_dac_enable: false,
            reg_volume: 0,
            reg_frequency: 0,
            wave_ram: [0; 16],
            sample: 0,
        }
    }

    fn trigger(&mut self) {
        // @todo - Triggering while reading sample from wave ram corrupts the data
        self.is_enabled = self.reg_dac_enable;

        // Trigger event: Frequency timer is reloaded with period.
        // self.freq_timer = 6; // @todo - Check blargg test 09 wave read while on
        self.freq_timer = (2048 - self.reg_frequency) * 2;

        // Trigger event: Wave channel's position is set to 0 but sample buffer is NOT refilled.
        self.sample_index = 0;
    }

    pub fn write_nr30(&mut self, data: u8) {
        self.reg_dac_enable = data & 0x80 != 0;

        if !self.reg_dac_enable {
            self.is_enabled = false;
        }
    }

    pub fn write_nr31(&mut self, data: u8) {
        self.length_counter.set_count(LENGTH_COUNTER_INIT - u16::from(data));
    }

    pub fn write_nr32(&mut self, data: u8) {
        self.reg_volume = (data & 0x60) >> 5;
    }

    pub fn write_nr33(&mut self, data: u8) {
        // @todo Period changes (written to NR33 or NR34) only take effect after the following time wave RAM is read.
        // https://gbdev.io/pandocs/Audio_Registers.html#ff1d--nr33-channel-3-period-low-write-only
        self.reg_frequency = (self.reg_frequency & 0x700) | u16::from(data);
    }

    pub fn write_nr34(&mut self, data: u8) {
        let length_enable_bit = data & 0x40 != 0;
        let trigger_bit = data & 0x80 != 0;
    
        self.reg_frequency = ((u16::from(data) & 0x7) << 8) | (self.reg_frequency & 0xFF);

        self.length_counter.write_nrx4(trigger_bit, length_enable_bit);

        if self.length_counter.is_enabled() && self.length_counter.get_count() == 0 {
            self.is_enabled = false;
        } else if trigger_bit {
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
        return if self.reg_dac_enable {
            0x80
        } else {
            0x0
        } | 0x7F;
    }

    pub fn read_nr31(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr32(&mut self) -> u8 {
        (self.reg_volume << 5) | 0x9F
    }

    pub fn read_nr33(&mut self) -> u8 {
        0xFF
    }

    pub fn read_nr34(&mut self) -> u8 {
        let length_bit = if self.get_length_counter().is_enabled() {
            0x40
        } else {
            0
        };
        0xBF | length_bit
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

        self.freq_timer = (2048 - self.reg_frequency) * 2;

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

        wave_sample = match self.reg_volume {
            0 => wave_sample >> 4,
            vol_shift => wave_sample >> (vol_shift - 1),
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

    fn get_envelope(&mut self) -> Option<&mut super::envelope::Envelope> {
        None
    }

    fn is_enabled(&self) -> bool {
        self.is_enabled && self.reg_dac_enable
    }

    fn shutdown(&mut self) {
        self.is_enabled = false;
        self.reg_dac_enable = false;
        self.reg_frequency = 0;
        self.reg_volume = 0;
        self.sample_index = 0;
        self.length_counter.shutdown();
    }
}
