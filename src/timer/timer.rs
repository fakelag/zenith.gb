pub struct Timer {
    and_result: bool,
    tima_overflow: bool,
    tima_overflow_tstates: u8,

    // xxxxxess
    tac: u8,
    div: u16,
    tima: u8,
    tma: u8,
}

impl Timer {
    pub fn new() -> Timer {
        Self {
            and_result: false,
            tima_overflow: false,
            tima_overflow_tstates: 0,
            tac: 0,
            div: 0xABD4,
            tima: 0,
            tma: 0,
        }
    }

    pub fn step(&mut self, cycles_passed: u8) -> bool {
        let t_states_passed = u16::from(cycles_passed * 4);

        let timer_enable = self.tac & 0x4 != 0;
        let tac_low_2 = self.tac & 0x3;

        let mut div_next = self.div;
        let mut timer_interrupt = false;

        for _ in 1..=t_states_passed {
            div_next = div_next.wrapping_add(1);

            let div_bit = match tac_low_2 {
                0 => div_next & (1 << 9) != 0,
                1 => div_next & (1 << 3) != 0,
                2 => div_next & (1 << 5) != 0,
                3 => div_next & (1 << 7) != 0,
                _ => unreachable!(),
            };

            let and_result = div_bit & timer_enable;

            if self.tima_overflow {
                self.tima_overflow_tstates -= 1;

                if self.tima_overflow_tstates == 0 {
                    self.tima = self.tma;

                    timer_interrupt = true;

                    self.tima_overflow = false;
                }
            } else if self.and_result && !and_result {
                let tima_prev = self.tima;
                self.tima = self.tima.wrapping_add(1);

                if tima_prev == 0xFF {
                    self.tima_overflow = true;
                    self.tima_overflow_tstates = 4;
                }
            }

            self.and_result = and_result;
        }

        self.div = div_next;
        return timer_interrupt;
    }

    pub fn read_tac(&self) -> u8 {
        self.tac | 0xF8
    }

    pub fn write_tac(&mut self, data: u8) {
        self.tac = data & 0x7;
    }
    
    pub fn read_div(&self) -> u8 {
        (self.div >> 8) as u8
    }

    pub fn write_div(&mut self, _data: u8) {
        self.div = 0;
    }

    pub fn read_tima(&self) -> u8 {
        self.tima
    }

    pub fn write_tima(&mut self, data: u8) {
        self.tima = data;
    }

    pub fn read_tma(&self) -> u8 {
        self.tma
    }

    pub fn write_tma(&mut self, data: u8) {
        self.tma = data;
    }
}
