use std::fmt::{self, Display};

use crate::{emu::emu::Emu, util::*};
use super::inst_def;

pub const FLAG_C: u8 = 0x1;
pub const FLAG_H: u8 = 0x10;
pub const FLAG_N: u8 = 0x20;
pub const FLAG_Z: u8 = 0x40;

#[derive(Debug)]
pub struct CPU {
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub sp: u16,
    pub pc: u16,
    pub cycles: u64,
    pub branch_skipped: bool,

    pub ime: bool,
    pub ime_next: bool,
}

impl Display for CPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "CPU")?;
        writeln!(f, "af={value:#06x} [{value:016b}], a={high:#x?} [{high:08b}], f={low:#x?} [{low:08b}]",
            value = self.af, high = util::get_high(self.af), low = util::get_low(self.af))?;
        writeln!(f, "bc={value:#06x} [{value:016b}], b={high:#x?} [{high:08b}], c={low:#x?} [{low:08b}]",
            value = self.bc, high = util::get_high(self.bc), low = util::get_low(self.bc))?;
        writeln!(f, "de={value:#06x} [{value:016b}], d={high:#x?} [{high:08b}], e={low:#x?} [{low:08b}]",
            value = self.de, high = util::get_high(self.de), low = util::get_low(self.de))?;
        writeln!(f, "hl={value:#06x} [{value:016b}], h={high:#x?} [{high:08b}], l={low:#x?} [{low:08b}]",
            value = self.hl, high = util::get_high(self.hl), low = util::get_low(self.hl))?;
        writeln!(f, "sp={value:#06x} [{value:016b}]", value = self.sp)?;
        writeln!(f, "pc={value:#06x} [{value:016b}]", value = self.pc)?;
        writeln!(f, "Z={} N={} H={} C={}", self.get_flag(FLAG_Z), self.get_flag(FLAG_N), self.get_flag(FLAG_H), self.get_flag(FLAG_C))?;
        writeln!(f, "cycles={value}", value = self.cycles)?;
        Ok(())
    }
}

impl CPU {
    pub fn new() -> Self {
        Self {
            af: 0,
            bc: 0,
            de: 0,
            hl: 0,
            sp: 0,
            pc: 0x100,
            cycles: 0,
            branch_skipped: false,
            ime: false,
            ime_next: false,
        }
    }

    pub fn step(emu: &mut Emu) {
        // @todo - Interrupts
        emu.cpu.branch_skipped = false;

        let mut opcode = emu.bus_read(emu.cpu.pc);
        emu.cpu.pc += 1;

        let inst = if opcode == 0xCB {
            opcode = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;
            inst_def::get_instruction_cb(opcode)
        } else {
            inst_def::get_instruction(opcode)
        };

        println!("opcode={:#x?} [{:08b}]", opcode, opcode);

        (inst.exec)(emu, inst, opcode);

        let inst_cycles: u8 = if emu.cpu.branch_skipped { inst.cycles_skipped } else { inst.cycles };
        debug_assert!(inst_cycles != 0);
        emu.cpu.cycles += u64::from(inst_cycles);
    }

    pub fn write_r8(emu: &mut Emu, r8_encoded: u8, val: u8) {
        match r8_encoded {
            0x7 => { util::set_high(&mut emu.cpu.af, val); }
            0x6 => {
                // 0b110 writes to [HL] instead of a register
                // https://gbdev.io/pandocs/CPU_Instruction_Set.html
                emu.bus_write(emu.cpu.hl, val);
            }
            0x5 => { util::set_low(&mut emu.cpu.hl, val); }
            0x4 => { util::set_high(&mut emu.cpu.hl, val); }
            0x3 => { util::set_low(&mut emu.cpu.de, val); }
            0x2 => { util::set_high(&mut emu.cpu.de, val); }
            0x1 => { util::set_low(&mut emu.cpu.bc, val); }
            0x0 => { util::set_high(&mut emu.cpu.bc, val); }
            _ => { unreachable!() }
        }
    }

    pub fn read_r8(emu: &Emu, r8_encoded: u8) -> u8 {
        match r8_encoded {
            0x7 => { return util::get_high(emu.cpu.af); }
            0x6 => { return emu.bus_read(emu.cpu.hl); }
            0x5 => { return util::get_low(emu.cpu.hl); }
            0x4 => { return util::get_high(emu.cpu.hl); }
            0x3 => { return util::get_low(emu.cpu.de); }
            0x2 => { return util::get_high(emu.cpu.de); }
            0x1 => { return util::get_low(emu.cpu.bc); }
            0x0 => { return util::get_high(emu.cpu.bc); }
            _ => { unreachable!() }
        }
    }

    pub fn read_r16(emu: &mut Emu, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => emu.cpu.bc,
            0x1 => emu.cpu.de,
            0x2 => emu.cpu.hl,
            0x3 => emu.cpu.sp,
            _ => { unreachable!() }
        }
    }

    pub fn write_r16(emu: &mut Emu, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => emu.cpu.bc = val,
            0x1 => emu.cpu.de = val,
            0x2 => emu.cpu.hl = val,
            0x3 => emu.cpu.sp = val,
            _ => { unreachable!() }
        }
    }

    pub fn read_r16stk(emu: &mut Emu, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => emu.cpu.bc,
            0x1 => emu.cpu.de,
            0x2 => emu.cpu.hl,
            0x3 => emu.cpu.af,
            _ => { unreachable!() }
        }
    }

    pub fn write_r16stk(emu: &mut Emu, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => emu.cpu.bc = val,
            0x1 => emu.cpu.de = val,
            0x2 => emu.cpu.hl = val,
            0x3 => emu.cpu.af = val,
            _ => { unreachable!() }
        }
    }

    pub fn get_flag(&self, flag: u8) -> bool {
        return (util::get_low(self.af) & flag) != 0;
    }

    pub fn set_flag(&mut self, flag: u8, set: bool) {
        let val = util::get_low(self.af);

        if set {
            util::set_low(&mut self.af, val | flag);
        } else {
            util::set_low(&mut self.af, val & !flag);
        }
    }
}