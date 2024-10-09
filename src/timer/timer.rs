use crate::soc::{interrupt, soc};

#[derive(Debug)]
pub struct Timer {
    and_result: bool,
    tima_overflow: bool,

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
            tac: 0,
            div: 0xABCC,
            tima: 0,
            tma: 0,
        }
    }

    pub fn clock(&mut self, ctx: &mut soc::ClockContext) {
        if self.tima_overflow {
            self.tima = self.tma;
            self.tima_overflow = false;

            ctx.set_interrupt(interrupt::INTERRUPT_BIT_TIMER);
        }

        self.div = self.div.wrapping_add(4);
        self.check_increment();
    }

    fn check_increment(&mut self) {
        let timer_enable = self.tac & 0x4 != 0;
        let tac_low_2 = self.tac & 0x3;

        let div_bit = match tac_low_2 {
            0 => self.div & (1 << 9) != 0,
            1 => self.div & (1 << 3) != 0,
            2 => self.div & (1 << 5) != 0,
            3 => self.div & (1 << 7) != 0,
            _ => unreachable!(),
        };

        let and_result = div_bit && timer_enable;

        if self.and_result && !and_result {
            let (tima_next, of) = self.tima.overflowing_add(1);
            self.tima = tima_next;

            if of {
                self.tima_overflow = true;
            }
        }

        self.and_result = and_result;
    }

    pub fn read_tac(&self) -> u8 {
        self.tac | 0xF8
    }

    pub fn read_div(&self) -> u8 {
        (self.div >> 8) as u8
    }

    pub fn read_tima(&self) -> u8 {
        self.tima
    }

    pub fn read_tma(&self) -> u8 {
        self.tma
    }

    pub fn clock_write_tac(&mut self, data: u8, ctx: &mut soc::ClockContext) {
        self.clock(ctx);
        self.tac = data & 0x7;
        self.check_increment();
    }

    pub fn clock_write_div(&mut self, _data: u8, ctx: &mut soc::ClockContext) {
        self.clock(ctx);
        self.check_increment();
        self.div = 0;
    }

    pub fn clock_write_tima(&mut self, data: u8, ctx: &mut soc::ClockContext) {
        let of = self.tima_overflow;
        self.clock(ctx);

        if !of {
            self.tima_overflow = false;
            self.tima = data;
        }
    }

    pub fn clock_write_tma(&mut self, data: u8, ctx: &mut soc::ClockContext) {
        let of = self.tima_overflow;
        self.clock(ctx);
        self.tma = data;
        if of {
            self.tima = data;
        }
    }
}
