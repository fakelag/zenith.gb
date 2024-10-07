use crate::soc::{interrupt, soc};

#[derive(Debug)]
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
            div: 0xABCC,
            tima: 0,
            tma: 0,
        }
    }

    pub fn clock(&mut self, ctx: &mut soc::ClockContext) {
        let t_states_passed = 4;

        let timer_enable = self.tac & 0x4 != 0;
        let tac_low_2 = self.tac & 0x3;

        let mut div_next = self.div;

        for _ in 0..t_states_passed {
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

                    ctx.set_interrupt(interrupt::INTERRUPT_BIT_TIMER);

                    self.tima_overflow = false;
                }
            } else if self.and_result && !and_result {
                let (tima, of) = self.tima.overflowing_add(1);
                self.tima = tima;

                if of {
                    self.tima_overflow = true;

                    // @todo - Check overflow & general timer behavior
                    self.tima_overflow_tstates = 3;
                }
            }

            self.and_result = and_result;
        }

        self.div = div_next;
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
        /*
            @todo - TIMA interrupt canceling
            The reload of the TMA value as well as the interrupt request can be aborted by writing any value to TIMA during the four T-cycles until it is supposed to be reloaded.
            The TIMA register contains whatever value was written to it even after the 4 T-cycles have elapsed and no interrupt will be requested.

            If TIMA is written to on the same T-cycle on which the reload from TMA occurs the write is ignored and the value in TMA will be loaded into TIMA.
            However, if TMA is written to on the same T-cycle on which the reload occurs,
            TMA is updated before its value is loaded into TIMA, meaning the reload will be carried out with the new value.
        */
        // println!("tima write: {:?} {}", self, data);
        self.tima = data;
    }

    pub fn read_tma(&self) -> u8 {
        self.tma
    }

    pub fn write_tma(&mut self, data: u8) {
        self.tma = data;
    }
}
