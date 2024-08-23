use crate::emu::emu::Emu;
use crate::cpu::*;
use crate::util::util;

use super::inst_def::*;

pub fn opcode_nop(emu: &mut Emu, instr: &Instruction, opcode: u8) { }

pub fn opcode_ld(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match (instr.dst, instr.src) {
        (OperandKind::R8, OperandKind::R8) => {
            // 0b01xxxyyy
            let src_reg = opcode & 0x7;
            let dst_reg = (opcode >> 3) & 0x7;
            let val = cpu::CPU::read_r8(emu, src_reg);

            // note: reg2reg will never trigger 0x6 write to [hl]
            debug_assert!(dst_reg != 0x6);

            cpu::CPU::write_r8(emu, dst_reg, val);
        }
        (OperandKind::R8, OperandKind::Imm8) => {
            let dst_reg = (opcode >> 3) & 0x7;
            let n8 = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;

            debug_assert!(dst_reg != 0x6); // should only happen with 0x36
            cpu::CPU::write_r8(emu, dst_reg, n8);
        }
        (OperandKind::R8, OperandKind::R16_Addr) => {
            match opcode {
                0x0A /* LD A [BC] */ => {
                    let val = emu.bus_read(emu.cpu.bc);
                    util::set_high(&mut emu.cpu.af, val);
                }
                0x1A /* LD A [DE] */ => {
                    let val = emu.bus_read(emu.cpu.de);
                    util::set_high(&mut emu.cpu.af, val);
                }
                0x2A /* LD A [HL+] */ => {
                    let val = emu.bus_read(emu.cpu.hl);
                    util::set_high(&mut emu.cpu.af, val);
                    emu.cpu.hl += 1;
                }
                0x3A /* LD A [HL-] */ => {
                    let val = emu.bus_read(emu.cpu.hl);
                    util::set_high(&mut emu.cpu.af, val);
                    emu.cpu.hl -= 1;
                }
                _ => /* LD r8, [HL] */ {
                    debug_assert!([0x46, 0x4E, 0x56, 0x5E, 0x66, 0x6E, 0x7E].contains(&opcode));
                    
                    let src_reg = opcode & 0x7;

                    // src_reg is always 0x6 e.g [HL]
                    debug_assert!(src_reg == 0x6);

                    let dst_reg = (opcode >> 3) & 0x7;
                    let val = cpu::CPU::read_r8(emu, src_reg);
        
                    // note: r16addr2r8 will never trigger 0x6 write to [hl]
                    debug_assert!(dst_reg != 0x6);
        
                    cpu::CPU::write_r8(emu, dst_reg, val);
                }
            }
        }
        (OperandKind::R8, OperandKind::Imm16_Addr) => {
            debug_assert!(opcode == 0xFA /* LD A [a16] */);
            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;
            let val = emu.bus_read(util::value(msb, lsb));
            util::set_high(&mut emu.cpu.af, val);
        }
        (OperandKind::R16, OperandKind::R16) => {
            debug_assert!([0xF8, 0xF9].contains(&opcode));
            match opcode {
                0xF8 /* LD HL SP+ */ => {
                    let e: i8;
                    let val = emu.bus_read(emu.cpu.pc);

                    unsafe { e = std::mem::transmute::<u8, i8>(val); }
                    emu.cpu.pc += 1;

                    let sum = emu.cpu.sp.wrapping_add_signed(e.into());
                    emu.cpu.hl = sum;

                    emu.cpu.set_flag(cpu::FLAG_Z, false);
                    emu.cpu.set_flag(cpu::FLAG_N, false);
                    emu.cpu.set_flag(cpu::FLAG_H, ((emu.cpu.sp ^ u16::from(val) ^ sum) & 0x10) == 0x10);
                    emu.cpu.set_flag(cpu::FLAG_C, ((emu.cpu.sp ^ u16::from(val) ^ sum) & 0x100) == 0x100);
                }
                0xF9 /* LD SP HL  */=> {
                    emu.cpu.sp = emu.cpu.hl;
                }
                _ => unreachable!(),
            }
        }
        (OperandKind::R16, OperandKind::Imm16) => {
            debug_assert!([0x01, 0x11, 0x21, 0x31].contains(&opcode));
            let dst_reg = (opcode >> 4) & 0x3;

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            cpu::CPU::write_r16(emu, dst_reg, util::value(msb, lsb));
        }
        (OperandKind::R16_Addr, OperandKind::R8) => {
            match opcode {
                0x22 /* LD [HL+] A */ => {
                    emu.bus_write(emu.cpu.hl, util::get_high(emu.cpu.af));
                    emu.cpu.hl += 1;
                }
                0x32 /* LD [HL-] A */ => {
                    emu.bus_write(emu.cpu.hl, util::get_high(emu.cpu.af));
                    emu.cpu.hl -= 1;
                }
                0x02 /* LD [BC] A */ => {
                    emu.bus_write(emu.cpu.bc, util::get_high(emu.cpu.af));
                }
                0x12 /* LD [DE] A */ => {
                    emu.bus_write(emu.cpu.de, util::get_high(emu.cpu.af));
                }
                /* LD [HL], r8 */ _ => {
                    debug_assert!([0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x77].contains(&opcode));

                    let src_reg = opcode & 0x7;
                    let val = cpu::CPU::read_r8(emu, src_reg);

                    // dst_reg will always be 0x6
                    debug_assert!((opcode >> 3) & 0x7 == 0x6);
                    emu.bus_write(emu.cpu.hl, val);
                }
            }
        }
        (OperandKind::Imm16_Addr, OperandKind::R16) => {
            debug_assert!(opcode == 0x08);

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            let addr = util::value(msb, lsb);
            emu.bus_write(addr, util::get_low(emu.cpu.sp));
            emu.bus_write(addr + 1, util::get_high(emu.cpu.sp));
        }
        (OperandKind::R16_Addr, OperandKind::Imm8) => {
            debug_assert!(opcode == 0x36); // could technically be decoded same as OperandKind::R8, OperandKind::Imm8
            let val = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;
            cpu::CPU::write_r8(emu, 0x6, val);
        }
        (OperandKind::R8_Addr, OperandKind::R8) => {
            debug_assert!(opcode == 0xE2);
            let addr = u16::from(util::get_low(emu.cpu.bc)) | 0xFF00;
            emu.bus_write(addr, util::get_high(emu.cpu.af));
        }
        (OperandKind::Imm16_Addr, OperandKind::R8) => {
            debug_assert!(opcode == 0xEA);

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            let addr = util::value(msb, lsb);
            emu.bus_write(addr, util::get_high(emu.cpu.af));
        }
        _ => todo!("instruction encoding not implemented for {:#x?} {:?} <- {:?}", opcode, instr.dst, instr.src),
    }
}

pub fn opcode_inc(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x03, 0x04, 0x0C, 0x13, 0x14, 0x1C, 0x23, 0x24, 0x2C, 0x33, 0x34, 0x3C"); }
pub fn opcode_dec(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x05, 0x0B, 0x0D, 0x15, 0x1B, 0x1D, 0x25, 0x2B, 0x2D, 0x35, 0x3B, 0x3D"); }
pub fn opcode_rlca(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x07"); }
pub fn opcode_add(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x09, 0x19, 0x29, 0x39, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0xC6, 0xE8"); }
pub fn opcode_rrca(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x0F"); }
pub fn opcode_stop(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x10"); }
pub fn opcode_rla(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x17"); }
pub fn opcode_jr(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x18, 0x20, 0x28, 0x30, 0x38"); }
pub fn opcode_rra(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x1F"); }
pub fn opcode_daa(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x27"); }
pub fn opcode_cpl(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x2F"); }
pub fn opcode_scf(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x37"); }
pub fn opcode_ccf(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x3F"); }
pub fn opcode_halt(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x76"); }
pub fn opcode_adc(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0xCE"); }
pub fn opcode_sub(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0xD6"); }
pub fn opcode_sbc(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F, 0xDE"); }
pub fn opcode_and(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xE6"); }
pub fn opcode_xor(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xEE"); }
pub fn opcode_or(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xF6"); }
pub fn opcode_cp(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xFE"); }

pub fn opcode_ret(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    println!("{}", emu.cpu);
    todo!("0xC0, 0xC8, 0xC9, 0xD0, 0xD8");
}

pub fn opcode_pop(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC1, 0xD1, 0xE1, 0xF1"); }

pub fn opcode_jp(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    if opcode == 0xE9 /* JP HL */ {
        emu.cpu.pc = emu.cpu.hl;
        return;
    }

    let branch_taken = match opcode {
        0xC3 /* JP a16 */ => {
            true
        }
        _ => {
            let flag = (opcode >> 3) & 0x7;

            let cond = match flag {
                0x3 => emu.cpu.get_flag(cpu::FLAG_C),
                0x2 => !emu.cpu.get_flag(cpu::FLAG_C),
                0x1 => emu.cpu.get_flag(cpu::FLAG_Z),
                0x0 => !emu.cpu.get_flag(cpu::FLAG_Z),
                _ => unreachable!(),
            };

            cond
        }
    };

    if branch_taken {
        let low = emu.bus_read(emu.cpu.pc);
        let high = emu.bus_read(emu.cpu.pc + 1);
        emu.cpu.pc = util::value(high, low);
    } else {
        emu.cpu.jmp_skipped = true;
    }
}

pub fn opcode_call(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC4, 0xCC, 0xCD, 0xD4, 0xDC"); }
pub fn opcode_push(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC5, 0xD5, 0xE5, 0xF5"); }
pub fn opcode_rst(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC7, 0xCF, 0xD7, 0xDF, 0xE7, 0xEF, 0xF7, 0xFF"); }
pub fn opcode_prefix(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xCB"); }
pub fn opcode_illegal_d3(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xD3"); }
pub fn opcode_reti(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xD9"); }
pub fn opcode_illegal_db(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xDB"); }
pub fn opcode_illegal_dd(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xDD"); }
pub fn opcode_ldh(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE0, 0xF0"); }
pub fn opcode_illegal_e3(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE3"); }
pub fn opcode_illegal_e4(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE4"); }
pub fn opcode_illegal_eb(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xEB"); }
pub fn opcode_illegal_ec(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xEC"); }
pub fn opcode_illegal_ed(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xED"); }
pub fn opcode_di(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xF3"); }
pub fn opcode_illegal_f4(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xF4"); }
pub fn opcode_ei(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFB"); }
pub fn opcode_illegal_fc(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFC"); }
pub fn opcode_illegal_fd(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFD"); }