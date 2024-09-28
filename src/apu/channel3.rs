use crate::util::util;

pub struct Channel3 {
    freq_timer: u16,

    length_counter: u16,

    // @todo - Clock frame sequencer globally from APU @ 512 Hz
    // right now it clocks channel3 length counter @ 256 Hz instead
    /*
        Step   Length Ctr  Vol Env     Sweep
        ---------------------------------------
        0      Clock       -           -
        1      -           -           -
        2      Clock       -           Clock
        3      -           -           -
        4      Clock       -           -
        5      -           -           -
        6      Clock       -           Clock
        7      -           Clock       -
        ---------------------------------------
        Rate   256 Hz      64 Hz       128 Hz
    */
    frame_sequencer: u16,

    is_enabled: bool,
    index: usize,

    nr30: u8,
    nr31: u8,
    nr32: u8,
    nr33: u8,
    nr34: u8,

    wave_ram: [u8; 16],
    last_sample: u8,
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            freq_timer: 0,
            length_counter: 0,
            frame_sequencer: 16384,
            index: 1,
            is_enabled: false,
            nr30: 0x7F,
            nr31: 0xFF,
            nr32: 0x9F,
            nr33: 0xFF,
            nr34: 0xBF,
            wave_ram: [0; 16],
            last_sample: 0,
        }
    }

    pub fn step(&mut self) {
        // A timer generates an output clock every N input clocks, where N is the timer's period.
        // If a timer's rate is given as a frequency, its period is 4194304/frequency in Hz.
        // Each timer has an internal counter that is decremented on each input clock.
        // When the counter becomes zero, it is reloaded with the period and an output clock is generated.

        self.freq_timer = self.freq_timer.saturating_sub(1);

        if self.freq_timer == 0 {
            // The wave channel's frequency timer period is set to (2048-frequency)*2
            let frequency = util::value(self.nr34 & 0x7, self.nr33);
            self.freq_timer = (2048 - frequency) * 2;

            if !self.is_enabled() {
                self.last_sample = 0;
                return;
            }

            // @todo Position in wave ram
            let wave_sample_tuple: u8 = self.wave_ram[self.index / 2];
            let wave_sample = if self.index % 2 == 0 {
                wave_sample_tuple >> 4
            } else {
                wave_sample_tuple & 0xF
            };

            self.index = (self.index + 1) % 32;

            // @todo Volume

            self.last_sample = wave_sample;
        }

        self.frame_sequencer -= 1;

        if self.frame_sequencer == 0 {
            // Clocks at 256 Hz
            // (4_194_304 / 16384 = 256)
            self.frame_sequencer = 16384;

            if self.length_enabled() {
                self.length_counter -= 1;

                if self.length_counter == 0 {
                    self.length_counter = 256;
                    self.is_enabled = false;
                }
            }
        }
    }

    pub fn get_last_sample(&self) -> u8 {
        self.last_sample
    }

    fn dac_enabled(&self) -> bool {
        self.nr30 & 0x80 != 0
    }

    fn length_enabled(&self) -> bool {
        self.nr34 & 0x40 != 0
    }

    fn is_enabled(&self) -> bool {
        self.is_enabled && self.dac_enabled()
    }

    fn trigger(&mut self) {
        // @todo - Triggering while reading sample from wave ram corrupts the data
        self.is_enabled = self.dac_enabled();

        // self.freq_timer = 6; // @todo - Check blargg test 09 wave read while on
        // Trigger event: Frequency timer is reloaded with period.
        let frequency = util::value(self.nr34 & 0x7, self.nr33);
        self.freq_timer = (2048 - frequency) * 2;

        // Trigger event: Wave channel's position is set to 0 but sample buffer is NOT refilled.
        self.index = 1;

        // Trigger event: If length counter is zero, it is set to 64 (256 for wave channel).
        if self.length_counter == 0 {
            self.length_counter = 256;
        }

        // @todo Trigger event: Channel volume is reloaded from NRx2.
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
        // Writing a byte to NRx1 loads the counter with 64-data (256-data for wave channel). 
        self.length_counter = 256;
        self.nr31 = data;
    }

    pub fn write_nr32(&mut self, data: u8) {
        // @todo - Volume control
        let ro_bits = self.nr32 & 0x9F;
        self.nr32 = (data & 0x60) | ro_bits;
    }

    pub fn write_nr33(&mut self, data: u8) {
        // @todo Period changes (written to NR33 or NR34) only take effect after the following time wave RAM is read.
        // https://gbdev.io/pandocs/Audio_Registers.html#ff1d--nr33-channel-3-period-low-write-only
        self.nr33 = data;
    }

    pub fn write_nr34(&mut self, data: u8) {
        // @todo - Check length timer, update rest of nr34
        if data & 0x80 != 0 {
            self.trigger();
        }

        // @todo - Check length counter behavior with nr34 write
        // for now just reset it to 256 and keep everything as is
        self.length_counter = 256;

        self.nr34 = data & 0xC7;
    }

    pub fn write_wave_ram(&mut self, addr: usize, data: u8) {
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
        self.wave_ram[addr & 0xF]
    }

    // pub fn length() {

    // }

    // pub fn vol() {

    // }

    // pub fn sweep() {

    // }
}
