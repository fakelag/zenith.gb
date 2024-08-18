use crate::emu::emu::Emu;
use crate::cpu::*;
use crate::util::util;

use super::inst_def::*;

pub fn opcode_nop(emu: &mut Emu, instr: &Instruction, opcode: u8) { emu.cpu.cycles += u64::from(instr.2); }
pub fn opcode_ld(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match instr.1 {
        OperandKind::R8 => {
            // 0b01xxxyyy
            match instr.0 {
                OperandKind::R8 => {
                    let src_reg = opcode & 0x7;
                    let val = cpu::CPU::read_r8(emu, src_reg);
                    let dst_reg = (opcode >> 3) & 0x7;

                    // note: reg2reg will never trigger 0x6 write to [hl]
                    cpu::CPU::write_r8(emu, dst_reg, val);
                }
                OperandKind::R16_Addr => {
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
                            let src_reg = opcode & 0x7;
                            let val = cpu::CPU::read_r8(emu, src_reg);
                            // let dst_reg = (opcode >> 3) & 0x7; // dst_reg will always be 0x6
                            emu.bus_write(emu.cpu.hl, val);
                        }
                    }
                }
                _ => { todo!("r8 -> instr.1") }
            }
        }
        _ => todo!("instr.0 -> any")
    }

    emu.cpu.cycles += u64::from(instr.2);
    return;

    // 0x0E | 0x1E | 0x2E | 0x3E | 0x06 | 0x16 | 0x26 | 0x36

    /*
    /* 0x01 LD BC n16    | - - - - */  Instruction(OperandKind::R16,        OperandKind::Imm16,      3,   opcode_ld),
    /* 0x02 LD [BC] A    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x06 LD B n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x08 LD [a16] SP  | - - - - */  Instruction(OperandKind::Imm16_Addr, OperandKind::R16,        5,   opcode_ld),
    /* 0x0A LD A [BC]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x0E LD C n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x11 LD DE n16    | - - - - */  Instruction(OperandKind::R16,        OperandKind::Imm16,      3,   opcode_ld),
    /* 0x12 LD [DE] A    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x16 LD D n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x1A LD A [DE]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x1E LD E n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x21 LD HL n16    | - - - - */  Instruction(OperandKind::R16,        OperandKind::Imm16,      3,   opcode_ld),
    /* 0x22 LD [HL+] A   | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x26 LD H n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x2A LD A [HL+]   | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x2E LD L n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x31 LD SP n16    | - - - - */  Instruction(OperandKind::R16,        OperandKind::Imm16,      3,   opcode_ld),
    /* 0x32 LD [HL-] A   | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x36 LD [HL] n8   | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::Imm8,       3,   opcode_ld),
    /* 0x3A LD A [HL-]   | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x3E LD A n8      | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm8,       2,   opcode_ld),
    /* 0x46 LD B [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x4E LD C [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x56 LD D [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x5E LD E [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x66 LD H [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x6E LD L [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0x70 LD [HL] B    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x71 LD [HL] C    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x72 LD [HL] D    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x73 LD [HL] E    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x74 LD [HL] H    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x75 LD [HL] L    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x77 LD [HL] A    | - - - - */  Instruction(OperandKind::R16_Addr,   OperandKind::R8,         2,   opcode_ld),
    /* 0x7E LD A [HL]    | - - - - */  Instruction(OperandKind::R8,         OperandKind::R16_Addr,   2,   opcode_ld),
    /* 0xE2 LD [C] A     | - - - - */  Instruction(OperandKind::R8_Addr,    OperandKind::R8,         2,   opcode_ld),
    /* 0xEA LD [a16] A   | - - - - */  Instruction(OperandKind::Imm16_Addr, OperandKind::R8,         4,   opcode_ld),
    /* 0xF8 LD HL SP+    | 0 0 H C */  Instruction(OperandKind::R16,        OperandKind::R16,        3,   opcode_ld),
    /* 0xF9 LD SP HL     | - - - - */  Instruction(OperandKind::R16,        OperandKind::R16,        2,   opcode_ld),
    /* 0xFA LD A [a16]   | - - - - */  Instruction(OperandKind::R8,         OperandKind::Imm16_Addr, 4,   opcode_ld),
    */
    
    todo!("0x01, 0x02, 0x06, 0x08, 0x0A, 0x0E, 0x11, 0x12, 0x16, 0x1A,
           0x1E, 0x21, 0x22, 0x26, 0x2A, 0x2E, 0x31, 0x32, 0x36, 0x3A,
           0x3E, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
           0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52,
           0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C,
           0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
           0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70,
           0x71, 0x72, 0x73, 0x74, 0x75, 0x77, 0x78, 0x79, 0x7A, 0x7B,
           0x7C, 0x7D, 0x7E, 0x7F, 0xE2, 0xEA, 0xF2, 0xF8, 0xF9, 0xFA");
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
pub fn opcode_ret(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC0, 0xC8, 0xC9, 0xD0, 0xD8"); }
pub fn opcode_pop(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC1, 0xD1, 0xE1, 0xF1"); }

pub fn opcode_jp(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    /*
        /* 0xC2 JP NZ a16    | - - - - */  Instruction(OperandKind::Flag_NZ,    OperandKind::Imm16_Addr, 4,   opcode_jp),
        /* 0xC3 JP a16       | - - - - */  Instruction(OperandKind::Imm16_Addr, OperandKind::None,       4,   opcode_jp),
        /* 0xCA JP Z a16     | - - - - */  Instruction(OperandKind::Flag_Z,     OperandKind::Imm16_Addr, 4,   opcode_jp),
        /* 0xD2 JP NC a16    | - - - - */  Instruction(OperandKind::Flag_NC,    OperandKind::Imm16_Addr, 4,   opcode_jp),
        /* 0xDA JP C a16     | - - - - */  Instruction(OperandKind::Flag_C,     OperandKind::Imm16_Addr, 4,   opcode_jp),
        /* 0xE9 JP HL        | - - - - */  Instruction(OperandKind::R16,        OperandKind::None,       1,   opcode_jp),
    */

    if opcode == 0xE9 /* JP HL */ {
        emu.cpu.pc = emu.cpu.hl;
        emu.cpu.cycles += u64::from(instr.2);
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
        emu.cpu.cycles += u64::from(instr.2);
    } else {
        // Only take 3 cycles if branch is skipped
        emu.cpu.cycles += 3;
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