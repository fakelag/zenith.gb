use std::{fmt::{self, Display}, time};

use crate::{cpu::cpu, mmu::mmu::MMU, util::util};

// const DIV_PER_TCYCLES: u32 = 4194304 / 16384;

pub struct Timer {
    and_result: bool,
    tima_overflow: bool,
    tima_overflow_tstates: u8,
}

impl Display for Timer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

impl Timer {
    pub fn new() -> Timer {
        Self { and_result: false, tima_overflow: false, tima_overflow_tstates: 0 }
    }

    pub fn step(&mut self, mmu: &mut MMU, cycles_passed: u8) {
        let t_states_passed = u16::from(cycles_passed * 4);

        let timer_enable = mmu.tac().check_bit(2);
        let tac_low_2 = mmu.tac().get() & 0x3;

        let div_lsb = mmu.div_lsb().get();
        let div_msb = mmu.div().get();
        let mut div_next = util::value(div_msb, div_lsb);

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
                    let tma = mmu.tma().get();
                    mmu.tima().set(tma);

                    let flags_if = mmu.r#if().get();
                    mmu.r#if().set(flags_if | cpu::INTERRUPT_BIT_TIMER);

                    self.tima_overflow = false;
                }
            } else if self.and_result && !and_result {
                let tima_prev = mmu.tima().inc();

                if tima_prev == 0xFF {
                    self.tima_overflow = true;
                    self.tima_overflow_tstates = 4;
                    self.and_result = and_result;
                    break;
                }
            }

            self.and_result = and_result;
        }

        mmu.div_lsb().set(util::get_low(div_next));
        mmu.div().set(util::get_high(div_next));
    }
}
