use std::fmt::{self, Display};

use crate::{util::*, mmu::mmu::MMU};
use super::inst_def;

pub const FLAG_C: u8 = 1 << 4;
pub const FLAG_H: u8 = 1 << 5;
pub const FLAG_N: u8 = 1 << 6;
pub const FLAG_Z: u8 = 1 << 7;

pub const HREG_IE: u16 = 0xFFFF;
pub const HREG_IF: u16 = 0xFF0F;

pub const INTERRUPT_BIT_VBLANK: u8  = 1 << 0;
pub const INTERRUPT_BIT_LCD: u8     = 1 << 1;
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
    pub fn new() -> CPU {
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

    
    pub fn step(&mut self, mmu: &mut MMU) -> u8 {
        self.check_interrupts(mmu);

        if self.halted {
            return 1;
        }

        let mut opcode = mmu.bus_read(self.pc);
        self.pc += 1;

        let inst = if opcode == 0xCB {
            opcode = mmu.bus_read(self.pc);
            self.pc += 1;
            inst_def::get_instruction_cb(opcode)
        } else {
            inst_def::get_instruction(opcode)
        };

        (inst.exec)(self, mmu, inst, opcode);

        let inst_cycles: u8 = if self.branch_skipped { inst.cycles_skipped } else { inst.cycles };
        debug_assert!(inst_cycles != 0);

        self.cycles += u64::from(inst_cycles);
        self.branch_skipped = false;

        inst_cycles
    }

    
    fn check_interrupts(&mut self, mmu: &mut MMU) {
        let ie_flags = mmu.bus_read(HREG_IE);
        let if_flags = mmu.bus_read(HREG_IF);

        let active_interrupts = ie_flags & if_flags;

        if active_interrupts == 0x0 {
            return;
        }

        // https://gbdev.io/pandocs/halt.html
        // The CPU wakes up as soon as an interrupt is pending, that is, when the bitwise AND of IE and IF is non-zero.
        self.halted = false;

        if !self.ime {
            return;
        }

        if self.handle_interrupt(mmu, active_interrupts, INTERRUPT_BIT_VBLANK, INTERRUPT_ADDR_VBLANK) {
            return;
        }
        if self.handle_interrupt(mmu, active_interrupts, INTERRUPT_BIT_LCD, INTERRUPT_ADDR_LCD) {
            return;
        }
        if self.handle_interrupt(mmu, active_interrupts, INTERRUPT_BIT_TIMER, INTERRUPT_ADDR_TIMER) {
            return;
        }
        if self.handle_interrupt(mmu, active_interrupts, INTERRUPT_BIT_SERIAL, INTERRUPT_ADDR_SERIAL) {
            return;
        }
        if self.handle_interrupt(mmu, active_interrupts, INTERRUPT_BIT_JOYPAD, INTERRUPT_ADDR_JOYPAD) {
            return;
        }
    }

    fn handle_interrupt(&mut self, mmu: &mut MMU, active_interrupts: u8, interrupt_bit: u8, interrupt_vec: u16) -> bool {
        if active_interrupts & interrupt_bit == 0 {
            return false;
        }

        self.push_u16(mmu, self.pc);
        self.pc = interrupt_vec;
        self.cycles += 5;
        self.ime = false;

        let if_flags = mmu.bus_read(HREG_IF);
        mmu.bus_write(HREG_IF, if_flags & !interrupt_bit);
        return true;
    }

    pub fn write_r8(&mut self, mmu: &mut MMU, r8_encoded: u8, val: u8) {
        match r8_encoded {
            0x7 => { util::set_high(&mut self.af, val); }
            0x6 => {
                // 0b110 writes to [HL] instead of a register
                // https://gbdev.io/pandocs/CPU_Instruction_Set.html
                mmu.bus_write(self.hl, val);
            }
            0x5 => { util::set_low(&mut self.hl, val); }
            0x4 => { util::set_high(&mut self.hl, val); }
            0x3 => { util::set_low(&mut self.de, val); }
            0x2 => { util::set_high(&mut self.de, val); }
            0x1 => { util::set_low(&mut self.bc, val); }
            0x0 => { util::set_high(&mut self.bc, val); }
            _ => { unreachable!() }
        }
    }
    
    pub fn read_r8(&mut self, mmu: &mut MMU, r8_encoded: u8) -> u8 {
        match r8_encoded {
            0x7 => { return util::get_high(self.af); }
            0x6 => { return mmu.bus_read(self.hl); }
            0x5 => { return util::get_low(self.hl); }
            0x4 => { return util::get_high(self.hl); }
            0x3 => { return util::get_low(self.de); }
            0x2 => { return util::get_high(self.de); }
            0x1 => { return util::get_low(self.bc); }
            0x0 => { return util::get_high(self.bc); }
            _ => { unreachable!() }
        }
    }
    
    pub fn read_r16(&mut self, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => self.bc,
            0x1 => self.de,
            0x2 => self.hl,
            0x3 => self.sp,
            _ => { unreachable!() }
        }
    }
    
    pub fn write_r16(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc = val,
            0x1 => self.de = val,
            0x2 => self.hl = val,
            0x3 => self.sp = val,
            _ => { unreachable!() }
        }
    }
    
    pub fn read_r16stk(&mut self, r16_encoded: u8) -> u16 {
        match r16_encoded {
            0x0 => self.bc,
            0x1 => self.de,
            0x2 => self.hl,
            0x3 => self.af,
            _ => { unreachable!() }
        }
    }
    
    pub fn write_r16stk(&mut self, r16_encoded: u8, val: u16) {
        match r16_encoded {
            0x0 => self.bc = val,
            0x1 => self.de = val,
            0x2 => self.hl = val,
            0x3 => self.af = val,
            _ => { unreachable!() }
        }
    }
    
    pub fn push_u16(&mut self, mmu: &mut MMU, val: u16) {
        self.sp -= 1;
        mmu.bus_write(self.sp, util::get_high(val));
        self.sp -= 1;
        mmu.bus_write(self.sp, util::get_low(val));
    }
    
    pub fn pop_u16(&mut self, mmu: &mut MMU) -> u16 {
        let lsb = mmu.bus_read(self.sp);
        self.sp += 1;
        let msb = mmu.bus_read(self.sp);
        self.sp += 1;
        return util::value(msb, lsb);
    }

    pub fn consume_byte_from_pc(&mut self, mmu: &mut MMU) -> u8 {
        let val = mmu.bus_read(self.pc);
        self.pc += 1;
        return val;
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
