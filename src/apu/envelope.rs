pub struct Envelope {
    period_timer: u8,
    enabled: bool,

    volume: u8,
    initial_volume: u8,
    add_mode: bool,
    period: u8,
}

impl Envelope {
    pub fn new(initial_volume: u8, add_mode: bool, period: u8) -> Self {
        Self {
            enabled: false,
            period_timer: 0,
            add_mode,
            period,
            initial_volume,
            volume: 0,
        }
    }

    pub fn clock(&mut self) {
        if self.period == 0 || !self.enabled {
            return;
        }

        self.period_timer -= 1;

        if self.period_timer != 0 {
            return;
        }

        self.reload_timer();

        if self.add_mode {
            if self.volume < 15 {
                self.volume += 1;
            }
        } else {
            if self.volume > 0 {
                self.volume -= 1;
            }
        };

        if self.volume == 0 || self.volume == 15 {
            self.enabled = false;
        }
    }

    pub fn write_nrx2(&mut self, data: u8) {
        self.initial_volume = (data >> 4) & 0xF;
        self.add_mode = data & 0x8 != 0;
        self.period = data & 0x7;
    }

    pub fn read_nrx2(&mut self) -> u8 {
        self.initial_volume << 4 | if self.add_mode { 1 << 3 } else { 0 } | self.period
    }

    pub fn trigger(&mut self) {
        self.enabled = true;
        self.volume = self.initial_volume;
        self.reload_timer();
    }

    pub fn get_volume(&self) -> u8 {
        return if self.period == 0 {
            self.initial_volume
        } else {
            self.volume
        };
    }

    pub fn shutdown(&mut self) {
        self.period_timer = 0;
        self.enabled = false;
        self.volume = 0;
        self.initial_volume = 0;
        self.add_mode = false;
        self.period = 0;
    }

    fn reload_timer(&mut self) {
        self.period_timer = if self.period == 0 { 8 } else { self.period };
    }
}
