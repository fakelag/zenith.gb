use std::{
    fmt::{self, Display},
    sync::mpsc::Sender,
};

use super::{
    inst_def,
    register::{Reg16b, Reg8b},
};
use crate::{
    soc::{interrupt::*, soc::SOC},
    util::*,
};

pub const FLAG_C: u8 = 1 << 4;
pub const FLAG_H: u8 = 1 << 5;
pub const FLAG_N: u8 = 1 << 6;
pub const FLAG_Z: u8 = 1 << 7;

const INTERRUPT_ADDR_VBLANK: u16 = 0x40;
const INTERRUPT_ADDR_LCD: u16 = 0x48;
const INTERRUPT_ADDR_TIMER: u16 = 0x50;
const INTERRUPT_ADDR_SERIAL: u16 = 0x58;
const INTERRUPT_ADDR_JOYPAD: u16 = 0x60;

const INTERRUPT_CYCLES: u8 = 5;

pub struct CPU {
    pub reg_af: u16,
    pub reg_bc: u16,
    pub reg_de: u16,
    pub reg_hl: u16,
    pub reg_sp: u16,
    pub reg_pc: u16,
    pub cycles: u64,
    pub branch_skipped: bool,

    pub ime_next: bool,
    pub ime: bool,
    pub halted: bool,

    pub ld_bb_breakpoint: Option<Sender<u8>>,
}

impl Display for CPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "CPU")?;
        writeln!(
            f,
            "af={value:#06x} [{value:016b}], a={high:#x?} [{high:08b}], f={low:#x?} [{low:08b}]",
            value = self.reg_af,
            high = util::get_high(self.reg_af),
            low = util::get_low(self.reg_af)
        )?;
        writeln!(
            f,
            "bc={value:#06x} [{value:016b}], b={high:#x?} [{high:08b}], c={low:#x?} [{low:08b}]",
            value = self.reg_bc,
            high = util::get_high(self.reg_bc),
            low = util::get_low(self.reg_bc)
        )?;
        writeln!(
            f,
            "de={value:#06x} [{value:016b}], d={high:#x?} [{high:08b}], e={low:#x?} [{low:08b}]",
            value = self.reg_de,
            high = util::get_high(self.reg_de),
            low = util::get_low(self.reg_de)
        )?;
        writeln!(
            f,
            "hl={value:#06x} [{value:016b}], h={high:#x?} [{high:08b}], l={low:#x?} [{low:08b}]",
            value = self.reg_hl,
            high = util::get_high(self.reg_hl),
            low = util::get_low(self.reg_hl)
        )?;
        writeln!(f, "sp={value:#06x} [{value:016b}]", value = self.reg_sp)?;
        writeln!(f, "pc={value:#06x} [{value:016b}]", value = self.reg_pc)?;
        writeln!(
            f,
            "Z={} N={} H={} C={}",
            self.get_flag(FLAG_Z),
            self.get_flag(FLAG_N),
            self.get_flag(FLAG_H),
            self.get_flag(FLAG_C)
        )?;
        writeln!(f, "cycles={value}", value = self.cycles)?;
        Ok(())
    }
}

impl CPU {
    pub fn new() -> CPU {
        Self {
            reg_af: 0,
            reg_bc: 0,
            reg_de: 0,
            reg_hl: 0,
            reg_sp: 0,
            reg_pc: 0,
            cycles: 0,
            branch_skipped: false,
            ime: false,
            ime_next: false,
            halted: false,
            ld_bb_breakpoint: None,
        }
    }

    pub fn step(&mut self, soc: &mut SOC) -> u8 {
        let intr_cycles = self.check_interrupts(soc);

        if self.ime_next {
            self.ime = true;
            self.ime_next = false;
        }

        if self.halted {
            return 1;
        }

        let mut opcode = self.consume_byte_from_pc(soc);

        let inst = if opcode == 0xCB {
            opcode = self.consume_byte_from_pc(soc);
            inst_def::get_instruction_cb(opcode)
        } else {
            inst_def::get_instruction(opcode)
        };

        (inst.exec)(self, soc, inst, opcode);

        let inst_cycles: u8 = if self.branch_skipped {
            inst.cycles_skipped
        } else {
            inst.cycles
        };
        debug_assert!(inst_cycles != 0);

        let total_cycles = inst_cycles + intr_cycles;

        self.cycles += u64::from(total_cycles);
        self.branch_skipped = false;

        total_cycles
    }

    fn check_interrupts(&mut self, soc: &mut SOC) -> u8 {
        let active_interrupts = soc.active_interrupts();

        if active_interrupts == 0x0 {
            return 0;
        }

        // https://gbdev.io/pandocs/halt.html
        // The CPU wakes up as soon as an interrupt is pending, that is, when the bitwise AND of IE and IF is non-zero.
        self.halted = false;

        if !self.ime {
            return 0;
        }

        if self.handle_interrupt(soc, INTERRUPT_BIT_VBLANK, INTERRUPT_ADDR_VBLANK) {
            return INTERRUPT_CYCLES;
        }
        if self.handle_interrupt(soc, INTERRUPT_BIT_LCD, INTERRUPT_ADDR_LCD) {
            return INTERRUPT_CYCLES;
        }
        if self.handle_interrupt(soc, INTERRUPT_BIT_TIMER, INTERRUPT_ADDR_TIMER) {
            return INTERRUPT_CYCLES;
        }
        if self.handle_interrupt(soc, INTERRUPT_BIT_SERIAL, INTERRUPT_ADDR_SERIAL) {
            return INTERRUPT_CYCLES;
        }
        if self.handle_interrupt(soc, INTERRUPT_BIT_JOYPAD, INTERRUPT_ADDR_JOYPAD) {
            return INTERRUPT_CYCLES;
        }

        return 0;
    }

    fn handle_interrupt(&mut self, soc: &mut SOC, interrupt_bit: u8, interrupt_vec: u16) -> bool {
        if soc.active_interrupts() & interrupt_bit == 0 {
            return false;
        }

        let pc_val = self.pc().get();

        // Note: push pc is done in 2 parts to allow interrupt canceling
        self.sp().dec();
        soc.bus_write(self.sp().get(), util::get_high(pc_val));

        let fire_interrupt = soc.active_interrupts() & interrupt_bit != 0;

        self.sp().dec();
        soc.bus_write(self.sp().get(), util::get_low(pc_val));

        if fire_interrupt {
            self.pc().set(interrupt_vec);
            soc.clear_interrupt(interrupt_bit);
        } else {
            // Interrupt canceling - push pc overwrote interrupt flags,
            // PC will be set to 0x0000 instead
            self.pc().set(0x0);
        }

        self.ime = false;

        return fire_interrupt;
    }

    pub fn write_r8(&mut self, soc: &mut SOC, r8_encoded: u8, val: u8) {
        match r8_encoded {
            0x7 => {
                return self.a().set(val);
            }
            0x6 => {
                // 0b110 writes to [HL] instead of a register
                // https://gbdev.io/pandocs/CPU_Instruction_Set.html
                soc.bus_write(self.hl().get(), val);
            }
            0x5 => {
                return self.l().set(val);
            }
            0x4 => {
                return self.h().set(val);
            }
            0x3 => {
                return self.e().set(val);
            }
            0x2 => {
                return self.d().set(val);
            }
            0x1 => {
                return self.c().set(val);
            }
            0x0 => {
                return self.b().set(val);
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub fn read_r8(&mut self, soc: &mut SOC, r8_encoded: u8) -> u8 {
        match r8_encoded {
            0x7 => {
                return self.a().get();
            }
            0x6 => {
                return soc.bus_read(self.hl().get());
            }
            0x5 => {
                return self.l().get();
            }
            0x4 => {
                return self.h().get();
            }
            0x3 => {
                return self.e().get();
            }
            0x2 => {
                return self.d().get();
            }
            0x1 => {
                return self.c().get();
            }
            0x0 => {
                return self.b().get();
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub fn read_r16(&mut self, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => self.bc().get(),
            0x1 => self.de().get(),
            0x2 => self.hl().get(),
            0x3 => self.sp().get(),
            _ => {
                unreachable!()
            }
        }
    }

    pub fn write_r16(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc().set(val),
            0x1 => self.de().set(val),
            0x2 => self.hl().set(val),
            0x3 => self.sp().set(val),
            _ => {
                unreachable!()
            }
        }
    }

    pub fn read_r16stk(&mut self, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => self.bc().get(),
            0x1 => self.de().get(),
            0x2 => self.hl().get(),
            0x3 => self.af().get(),
            _ => {
                unreachable!()
            }
        }
    }

    pub fn write_r16stk(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc().set(val),
            0x1 => self.de().set(val),
            0x2 => self.hl().set(val),
            0x3 => self.af().set(val & 0xFFF0),
            _ => {
                unreachable!()
            }
        }
    }

    pub fn push_u16(&mut self, soc: &mut SOC, val: u16) {
        self.sp().dec();
        soc.bus_write(self.sp().get(), util::get_high(val));
        self.sp().dec();
        soc.bus_write(self.sp().get(), util::get_low(val));
    }

    pub fn pop_u16(&mut self, soc: &mut SOC) -> u16 {
        let lsb = soc.bus_read(self.sp().inc());
        let msb = soc.bus_read(self.sp().inc());
        return util::value(msb, lsb);
    }

    pub fn consume_byte_from_pc(&mut self, soc: &mut SOC) -> u8 {
        let val = soc.bus_read(self.pc().inc());
        return val;
    }

    pub fn get_flag(&self, flag: u8) -> bool {
        return (util::get_low(self.reg_af) & flag) != 0;
    }

    pub fn set_flag(&mut self, flag: u8, set: bool) {
        let val = self.f().get();

        if set {
            self.f().set(val | flag);
        } else {
            self.f().set(val & !flag);
        }
    }

    pub fn a(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_af, true)
    }
    fn f(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_af, false)
    }
    pub fn b(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_bc, true)
    }
    pub fn c(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_bc, false)
    }
    pub fn d(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_de, true)
    }
    pub fn e(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_de, false)
    }
    pub fn h(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_hl, true)
    }
    pub fn l(&mut self) -> Reg8b {
        Reg8b::new(&mut self.reg_hl, false)
    }

    pub fn af(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_af)
    }
    pub fn bc(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_bc)
    }
    pub fn de(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_de)
    }
    pub fn hl(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_hl)
    }
    pub fn sp(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_sp)
    }
    pub fn pc(&mut self) -> Reg16b {
        Reg16b::new(&mut self.reg_pc)
    }

    #[cfg(test)]
    pub fn set_breakpoint(&mut self, bp_send: Option<Sender<u8>>) {
        self.ld_bb_breakpoint = bp_send;
    }
}
