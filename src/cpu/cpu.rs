use std::fmt::{self, Display};

use crate::{emu::emu::Emu, util::*};
use super::inst_def;

pub const FLAG_C: u8 = 1 << 4;
pub const FLAG_H: u8 = 1 << 5;
pub const FLAG_N: u8 = 1 << 6;
pub const FLAG_Z: u8 = 1 << 7;

pub const HREG_IE: u16 = 0xFFFF;
pub const HREG_IF: u16 = 0xFF0F;

pub const INTERRUPT_BIT_VBLANK: u8  = 1 << 0;
const INTERRUPT_BIT_LCD: u8     = 1 << 1;
const INTERRUPT_BIT_TIMER: u8   = 1 << 2;
const INTERRUPT_BIT_SERIAL: u8  = 1 << 3;
const INTERRUPT_BIT_JOYPAD: u8  = 1 << 4;

const INTERRUPT_ADDR_VBLANK: u16  = 0x40;
const INTERRUPT_ADDR_LCD: u16     = 0x48;
const INTERRUPT_ADDR_TIMER: u16   = 0x50;
const INTERRUPT_ADDR_SERIAL: u16  = 0x58;
const INTERRUPT_ADDR_JOYPAD: u16  = 0x60;

pub struct CPU {
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub sp: u16,
    pub pc: u16,
    pub cycles: u64,
    pub branch_skipped: bool,

    // @todo - Enabling interrupts should have one instruction delay:
    //  The effect of ei is delayed by one instruction.
    //  This means that ei followed immediately by di does not allow any interrupts between them.
    //  https://gbdev.io/pandocs/Interrupts.html
    pub ime: bool,
    pub halted: bool,
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
            pc: 0,
            cycles: 0,
            branch_skipped: false,
            ime: false,
            halted: false,
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

pub fn step(emu: &mut Emu) -> u8 {
    check_interrupts(emu);

    if emu.cpu.halted {
        return 1;
    }

    let mut opcode = emu.bus_read(emu.cpu.pc);
    emu.cpu.pc += 1;

    let inst = if opcode == 0xCB {
        opcode = emu.bus_read(emu.cpu.pc);
        emu.cpu.pc += 1;
        inst_def::get_instruction_cb(opcode)
    } else {
        inst_def::get_instruction(opcode)
    };

    (inst.exec)(emu, inst, opcode);

    let inst_cycles: u8 = if emu.cpu.branch_skipped { inst.cycles_skipped } else { inst.cycles };
    debug_assert!(inst_cycles != 0);

    emu.cpu.cycles += u64::from(inst_cycles);
    emu.cpu.branch_skipped = false;

    inst_cycles
}

fn check_interrupts(emu: &mut Emu) {
    let ie_flags = emu.bus_read(HREG_IE);
    let if_flags = emu.bus_read(HREG_IF);

    let active_interrupts = ie_flags & if_flags;

    if active_interrupts == 0x0 {
        return;
    }

    // https://gbdev.io/pandocs/halt.html
    // The CPU wakes up as soon as an interrupt is pending, that is, when the bitwise AND of IE and IF is non-zero.
    emu.cpu.halted = false;

    if !emu.cpu.ime {
        return;
    }

    if handle_interrupt(emu, active_interrupts, INTERRUPT_BIT_VBLANK, INTERRUPT_ADDR_VBLANK) {
        return;
    }
    if handle_interrupt(emu, active_interrupts, INTERRUPT_BIT_LCD, INTERRUPT_ADDR_LCD) {
        return;
    }
    if handle_interrupt(emu, active_interrupts, INTERRUPT_BIT_TIMER, INTERRUPT_ADDR_TIMER) {
        return;
    }
    if handle_interrupt(emu, active_interrupts, INTERRUPT_BIT_SERIAL, INTERRUPT_ADDR_SERIAL) {
        return;
    }
    if handle_interrupt(emu, active_interrupts, INTERRUPT_BIT_JOYPAD, INTERRUPT_ADDR_JOYPAD) {
        return;
    }
}

fn handle_interrupt(emu: &mut Emu, active_interrupts: u8, interrupt_bit: u8, interrupt_vec: u16) -> bool {
    if active_interrupts & interrupt_bit == 0 {
        return false;
    }

    push_u16(emu, emu.cpu.pc);
    emu.cpu.pc = interrupt_vec;
    emu.cpu.cycles += 5;
    emu.cpu.ime = false;

    let if_flags = emu.bus_read(HREG_IF);
    emu.bus_write(HREG_IF, if_flags & !interrupt_bit);
    return true;
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

pub fn push_u16(emu: &mut Emu, val: u16) {
    emu.cpu.sp -= 1;
    emu.bus_write(emu.cpu.sp, util::get_high(val));
    emu.cpu.sp -= 1;
    emu.bus_write(emu.cpu.sp, util::get_low(val));
}

pub fn pop_u16(emu: &mut Emu) -> u16 {
    let lsb = emu.bus_read(emu.cpu.sp);
    emu.cpu.sp += 1;
    let msb = emu.bus_read(emu.cpu.sp);
    emu.cpu.sp += 1;
    return util::value(msb, lsb);
}
