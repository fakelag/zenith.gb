use super::{lengthcounter::LengthCounter, Channel};

const LENGTH_COUNTER_INIT: u16 = 256;

pub struct Channel3 {
    freq_timer: u16,
    last_sample_step: u16,

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
            last_sample_step: 0,
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
        // Triggering the wave channel on the DMG while it reads a sample byte will alter the first four bytes of wave RAM.
        // If the channel was reading one of the first four bytes, the only first byte will be rewritten with the byte being read.
        // If the channel was reading one of the later 12 bytes, the first FOUR bytes of wave RAM will be rewritten with the four
        // aligned bytes that the read was from (bytes 4-7, 8-11, or 12-15); for example if it were reading byte 9 when it was retriggered,
        // the first four bytes would be rewritten with the contents of bytes 8-11.
        // To avoid this corruption you should stop the wave by writing 0 then $80 to NR30 before triggering it again.
        // The game Duck Tales encounters this issue part way through most songs.

        // @todo CGB: corruption occurs on DMG only
        if self.freq_timer == 2 && self.is_enabled() {
            let next_byte_index = (self.sample_index / 2) as u8;

            if next_byte_index < 4 {
                self.wave_ram[0] = self.wave_ram[next_byte_index as usize];
            } else {
                let aligned_index = (next_byte_index & 0xFC) as usize;
                for i in 0..4 {
                    self.wave_ram[i] = self.wave_ram[aligned_index + i];
                }
            }
        }

        self.is_enabled = self.reg_dac_enable;

        // Trigger event: Frequency timer is reloaded with period.
        self.freq_timer = 6;

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
               // @todo CGB: self.last_sample_step is irrelevant on CGB. Write to last_sample_index occurs always
            if self.last_sample_step < 2 {
                let last_sample_index = if self.sample_index == 0 { 31 } else { self.sample_index - 1 };
                self.wave_ram[last_sample_index / 2] = data;
            }
            return;
        }
        self.wave_ram[addr & 0xF] = data;
    }

    pub fn read_nr30(&mut self) -> u8 {
        let dac_bit = (self.reg_dac_enable as u8) << 7;
        dac_bit| 0x7F
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
        let length_bit = (self.length_counter.is_enabled() as u8) << 6;
        0xBF | length_bit
    }

    pub fn read_wave_ram(&mut self, addr: usize) -> u8 {
        if self.is_enabled() {
            // @todo CGB: self.last_sample_step is irrelevant on CGB. Read from last_sample_index occurs always
            if self.last_sample_step < 2 {
                let last_sample_index = if self.sample_index == 0 { 31 } else { self.sample_index - 1 };
                return self.wave_ram[last_sample_index / 2];
            }
            return 0xFF;
        }
        self.wave_ram[addr & 0xF]
    }
}

impl Channel for Channel3 {
    fn step(&mut self) {
        self.last_sample_step = self.last_sample_step.saturating_add(1);

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
        self.last_sample_step = 0;
    }

    fn length_step(&mut self) {
        if self.length_counter.step() {
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
