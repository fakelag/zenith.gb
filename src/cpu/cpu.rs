use std::{
    fmt::{self, Display},
    sync::mpsc::SyncSender,
};

use super::{
    inst_def,
    register::{Reg16b, Reg8b},
};
use crate::{
    cartridge::cartridge::Cartridge,
    soc::{interrupt::*, soc::SOC},
    util::*,
    CompatibilityMode,
};

pub type BpSender = SyncSender<(u16, u16, u16)>;

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

const INTERRUPT_BITS: [u8; 5] = [
    INTERRUPT_BIT_VBLANK,
    INTERRUPT_BIT_LCD,
    INTERRUPT_BIT_TIMER,
    INTERRUPT_BIT_SERIAL,
    INTERRUPT_BIT_JOYPAD,
];

const INTERRUPT_VECS: [u16; 5] = [
    INTERRUPT_ADDR_VBLANK,
    INTERRUPT_ADDR_LCD,
    INTERRUPT_ADDR_TIMER,
    INTERRUPT_ADDR_SERIAL,
    INTERRUPT_ADDR_JOYPAD,
];

pub struct CPU {
    opcode: u8,
    reg_af: u16,
    reg_bc: u16,
    reg_de: u16,
    reg_hl: u16,
    reg_sp: u16,
    reg_pc: u16,
    pub branch_skipped: bool,

    pub ime_next: bool,
    pub ime: bool,
    pub halted: bool,

    pub ld_bb_breakpoint: Option<BpSender>,
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
        Ok(())
    }
}

impl CPU {
    pub fn new(ld_bb_breakpoint: Option<BpSender>) -> CPU {
        Self {
            opcode: 0x0,
            reg_af: 0,
            reg_bc: 0,
            reg_de: 0,
            reg_hl: 0,
            reg_sp: 0,
            reg_pc: 0,
            branch_skipped: false,
            ime: false,
            ime_next: false,
            halted: false,
            ld_bb_breakpoint,
        }
    }

    pub fn init(&mut self, _soc: &mut SOC, cartridge: &Cartridge, mode: CompatibilityMode) {
        match mode {
            CompatibilityMode::CompCgb => {
                self.a().set(0x11);
                self.b().set(0x0);
                self.c().set(0x0);
                self.d().set(0xFF);
                self.e().set(0x56);
                self.h().set(0x0);
                self.l().set(0x0D);

                self.sp().set(0xFFFE);
                self.pc().set(0x100);

                self.set_flag(FLAG_Z, true);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, false);
            }
            CompatibilityMode::CompCgbDmg => {
                self.a().set(0x11);
                self.b().set(0x0);
                self.c().set(0x0);
                self.d().set(0x0);
                self.e().set(0x08);
                self.h().set(0x0);
                self.l().set(0x7C);

                self.sp().set(0xFFFE);
                self.pc().set(0x100);

                self.set_flag(FLAG_Z, true);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, false);
            }
            CompatibilityMode::CompDmg => {
                self.a().set(0x1);
                self.b().set(0);
                self.c().set(0x13);
                self.d().set(0);
                self.e().set(0xD8);
                self.h().set(0x1);
                self.l().set(0x4D);

                self.sp().set(0xFFFE);
                self.pc().set(0x100);

                self.set_flag(FLAG_Z, true);
                self.set_flag(FLAG_N, false);
                self.set_flag(
                    FLAG_H,
                    if cartridge.header.header_checksum == 0x0 {
                        false
                    } else {
                        true
                    },
                );
                self.set_flag(
                    FLAG_C,
                    if cartridge.header.header_checksum == 0x0 {
                        false
                    } else {
                        true
                    },
                );
            }
        }

        // Hack: next instruction fetch happens at the end of the previously executed instruction.
        // Prefetch first instruction from cartridge
        self.opcode = cartridge.data[0x100];
        self.pc().inc();
        // self.clock_fetch(soc);
    }

    pub fn step(&mut self, soc: &mut SOC) -> u64 {
        let soc_cycles = soc.cycles;

        let is_interrupt_cycle = self.check_interrupts(soc);

        if self.ime_next {
            self.ime = true;
            self.ime_next = false;
        }

        if self.halted {
            soc.clock();
            debug_assert!(soc.cycles - soc_cycles == 1);
            return soc.cycles - soc_cycles;
        }

        let inst = if self.opcode == 0xCB {
            self.opcode = self.clock_consume_byte_from_pc(soc);
            inst_def::get_instruction_cb(self.opcode)
        } else {
            inst_def::get_instruction(self.opcode)
        };

        (inst.exec)(self, soc, inst, self.opcode);

        self.clock_fetch(soc);

        // Verify that soc was clocked correct number of times
        debug_assert!(
            (((if self.branch_skipped {
                inst.cycles_skipped
            } else {
                inst.cycles
            }) + (is_interrupt_cycle as u8 * INTERRUPT_CYCLES)) as u64)
                == soc.cycles - soc_cycles
        );

        self.branch_skipped = false;

        soc.cycles - soc_cycles
    }

    fn clock_fetch(&mut self, soc: &mut SOC) {
        self.opcode = self.clock_consume_byte_from_pc(soc);
    }

    fn check_interrupts(&mut self, soc: &mut SOC) -> bool {
        let active_interrupts = soc.active_interrupts();

        if active_interrupts == 0x0 {
            return false;
        }

        // https://gbdev.io/pandocs/halt.html
        // The CPU wakes up as soon as an interrupt is pending, that is, when the bitwise AND of IE and IF is non-zero.
        self.halted = false;

        if !self.ime {
            return false;
        }

        return (0..INTERRUPT_BITS.len()).any(|i| {
            if soc.active_interrupts() & INTERRUPT_BITS[i] == 0 {
                return false;
            }
            self.handle_interrupt(soc);
            return true;
        });
    }

    fn handle_interrupt(&mut self, soc: &mut SOC) {
        soc.clock();
        soc.clock();

        let pc_val = self.pc().get() - 1;

        // Note: push pc is done in 2 parts to allow interrupt canceling
        self.clock_push_u8(soc, util::get_high(pc_val));

        let active_interrupt = (0..INTERRUPT_BITS.len()).into_iter().find_map(|index| {
            if soc.active_interrupts() & INTERRUPT_BITS[index] == 0 {
                return None;
            }
            return Some(index);
        });

        self.clock_push_u8(soc, util::get_low(pc_val));

        if let Some(interrupt_index) = active_interrupt {
            self.pc().set(INTERRUPT_VECS[interrupt_index]);
            soc.clear_interrupt(INTERRUPT_BITS[interrupt_index]);
        } else {
            // Interrupt canceling - push pc overwrote interrupt flags,
            // PC will be set to 0x0000 instead
            self.pc().set(0x0);
        }

        self.ime = false;

        self.clock_fetch(soc);
    }

    pub fn clock_write_at_hl(&mut self, soc: &mut SOC, val: u8) {
        soc.clock_write(self.hl().get(), val);
    }

    pub fn write_r8(&mut self, r8_encoded: u8, val: u8) {
        match r8_encoded {
            0x7 => self.a().set(val),
            0x6 => unreachable!("write_r8 should not access memory"),
            0x5 => self.l().set(val),
            0x4 => self.h().set(val),
            0x3 => self.e().set(val),
            0x2 => self.d().set(val),
            0x1 => self.c().set(val),
            0x0 => self.b().set(val),
            _ => unreachable!(),
        }
    }

    pub fn clock_read_at_hl(&mut self, soc: &mut SOC) -> u8 {
        soc.clock_read(self.hl().get())
    }

    pub fn read_r8(&mut self, r8_encoded: u8) -> u8 {
        match r8_encoded {
            0x7 => self.a().get(),
            0x6 => unreachable!("read_r8 should not access memory"),
            0x5 => self.l().get(),
            0x4 => self.h().get(),
            0x3 => self.e().get(),
            0x2 => self.d().get(),
            0x1 => self.c().get(),
            0x0 => self.b().get(),
            _ => unreachable!(),
        }
    }

    pub fn read_r16(&mut self, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => self.bc().get(),
            0x1 => self.de().get(),
            0x2 => self.hl().get(),
            0x3 => self.sp().get(),
            _ => unreachable!(),
        }
    }

    pub fn write_r16(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc().set(val),
            0x1 => self.de().set(val),
            0x2 => self.hl().set(val),
            0x3 => self.sp().set(val),
            _ => unreachable!(),
        }
    }

    pub fn read_r16stk_msb(&mut self, r16_encoded: u8) -> u8 {
        match r16_encoded {
            0x0 => self.b().get(),
            0x1 => self.d().get(),
            0x2 => self.h().get(),
            0x3 => self.a().get(),
            _ => unreachable!(),
        }
    }

    pub fn read_r16stk_lsb(&mut self, r16_encoded: u8) -> u8 {
        match r16_encoded {
            0x0 => self.c().get(),
            0x1 => self.e().get(),
            0x2 => self.l().get(),
            0x3 => self.f().get(),
            _ => unreachable!(),
        }
    }

    pub fn write_r16stk(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc().set(val),
            0x1 => self.de().set(val),
            0x2 => self.hl().set(val),
            0x3 => self.af().set(val & 0xFFF0),
            _ => unreachable!(),
        }
    }

    pub fn clock_push_u8(&mut self, soc: &mut SOC, val: u8) {
        self.sp().dec();
        soc.clock_write(self.sp().get(), val);
    }

    pub fn clock_pop_u16(&mut self, soc: &mut SOC) -> u16 {
        let lsb = soc.clock_read(self.sp().inc());
        let msb = soc.clock_read(self.sp().inc());
        return util::value(msb, lsb);
    }

    pub fn clock_consume_byte_from_pc(&mut self, soc: &mut SOC) -> u8 {
        let val = soc.clock_read(self.pc().inc());
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
}
